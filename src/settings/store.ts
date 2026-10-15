// App settings, written through to the `settings` table on every change.
// The pane layout lives in the same table but is owned by layout/persist.ts.
import { create } from "zustand";
import { db } from "../db";

export type ShellChoice = "auto" | "pwsh" | "powershell" | "cmd" | "wsl";
/** Plan-11: `system` follows the OS; the others pin `data-theme` on <html>. */
export type Theme = "system" | "dark" | "light";
/** Plan-18: where a new pane's proxy comes from. */
export type ProxyMode = "off" | "manual" | "system";

/** Mirrors `proxy::DEFAULT_BYPASS` in Rust. */
export const DEFAULT_BYPASS = "localhost,127.0.0.1,::1,.local";

export const SHELL_CHOICES: Array<{ value: ShellChoice; label: string }> = [
  { value: "auto", label: "Auto (pwsh, else powershell)" },
  { value: "pwsh", label: "pwsh" },
  { value: "powershell", label: "powershell" },
  { value: "cmd", label: "cmd" },
  { value: "wsl", label: "wsl" },
];

interface SettingsState {
  theme: Theme;
  shell: ShellChoice;
  restoreLayout: boolean;
  proxyMode: ProxyMode;
  /** Local proxy port for Manual mode; kept, not cleared, in the other modes. */
  proxyPort: string;
  /** `no_proxy` for new panes, comma separated; applies in every mode. */
  proxyBypass: string;
  /** Loads every key; returns the raw map so the caller can read `layout` too. */
  load(): Promise<Record<string, string>>;
  setTheme(theme: Theme): void;
  setShell(shell: ShellChoice): void;
  setRestoreLayout(on: boolean): void;
  setProxyMode(mode: ProxyMode): void;
  setProxyPort(port: string): void;
  setProxyBypass(bypass: string): void;
}

export const isTheme = (v: string | undefined): v is Theme => v === "system" || v === "dark" || v === "light";

/**
 * Puts the theme on <html> synchronously — `system` means no attribute, the
 * media query decides. Done here rather than in a React effect so that a
 * pane's own effect (children run first) already sees the new tokens.
 */
export function applyTheme(theme: Theme): void {
  if (typeof document === "undefined") return;
  if (theme === "system") document.documentElement.removeAttribute("data-theme");
  else document.documentElement.dataset.theme = theme;
}
const isProxyMode = (v: string | undefined): v is ProxyMode => v === "off" || v === "manual" || v === "system";
const isShell = (v: string | undefined): v is ShellChoice =>
  SHELL_CHOICES.some((c) => c.value === v);

function persist(key: string, value: string) {
  db.settingsSet(key, value).catch((e) => console.warn("settings_set", key, e));
}

export const useSettings = create<SettingsState>((set) => ({
  theme: "system",
  shell: "auto",
  restoreLayout: true,
  proxyMode: "system",
  proxyPort: "",
  proxyBypass: DEFAULT_BYPASS,

  async load() {
    const all = await db.settingsGetAll();
    applyTheme(isTheme(all.theme) ? all.theme : "system");
    // Before plan-18 a valid port was the only way to get a proxy: keep those
    // users on it. Saved once, so clearing the port later does not flip the mode.
    const savedMode = all.proxy_mode;
    const proxyMode: ProxyMode = isProxyMode(savedMode)
      ? savedMode
      : parsePort(all.proxy_port ?? "") === null ? "system" : "manual";
    if (proxyMode !== savedMode) persist("proxy_mode", proxyMode);
    set({
      theme: isTheme(all.theme) ? all.theme : "system",
      shell: isShell(all.shell) ? all.shell : "auto",
      restoreLayout: all.restore_layout !== "0",
      proxyMode,
      proxyPort: all.proxy_port ?? "",
      proxyBypass: all.proxy_bypass ?? DEFAULT_BYPASS,
    });
    return all;
  },

  setTheme(theme) {
    applyTheme(theme);
    set({ theme });
    persist("theme", theme);
  },
  setShell(shell) {
    set({ shell });
    persist("shell", shell);
  },
  setRestoreLayout(restoreLayout) {
    set({ restoreLayout });
    persist("restore_layout", restoreLayout ? "1" : "0");
  },
  setProxyMode(proxyMode) {
    set({ proxyMode });
    persist("proxy_mode", proxyMode);
  },
  setProxyPort(proxyPort) {
    set({ proxyPort });
    persist("proxy_port", proxyPort);
  },
  setProxyBypass(proxyBypass) {
    set({ proxyBypass });
    persist("proxy_bypass", proxyBypass);
  },
}));

/** The `shell` argument for pty_spawn: `undefined` lets Rust pick the default. */
export function shellForSpawn(choice: ShellChoice): string | undefined {
  return choice === "auto" ? undefined : `${choice}.exe`;
}

/** A usable TCP port, or null for anything else (blank, partial, out of range). */
export function parsePort(raw: string): number | null {
  const s = raw.trim();
  if (!/^\d{1,5}$/.test(s)) return null;
  const n = Number(s);
  return n >= 1 && n <= 65535 ? n : null;
}
