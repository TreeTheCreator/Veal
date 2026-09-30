// App settings, written through to the `settings` table on every change.
// The pane layout lives in the same table but is owned by layout/persist.ts.
import { create } from "zustand";
import { db } from "../db";

export type ShellChoice = "auto" | "pwsh" | "powershell" | "cmd" | "wsl";
/** Plan-11: `system` follows the OS; the others pin `data-theme` on <html>. */
export type Theme = "system" | "dark" | "light";

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
  /** Loads every key; returns the raw map so the caller can read `layout` too. */
  load(): Promise<Record<string, string>>;
  setTheme(theme: Theme): void;
  setShell(shell: ShellChoice): void;
  setRestoreLayout(on: boolean): void;
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
const isShell = (v: string | undefined): v is ShellChoice =>
  SHELL_CHOICES.some((c) => c.value === v);

function persist(key: string, value: string) {
  db.settingsSet(key, value).catch((e) => console.warn("settings_set", key, e));
}

export const useSettings = create<SettingsState>((set) => ({
  theme: "system",
  shell: "auto",
  restoreLayout: true,

  async load() {
    const all = await db.settingsGetAll();
    applyTheme(isTheme(all.theme) ? all.theme : "system");
    set({
      theme: isTheme(all.theme) ? all.theme : "system",
      shell: isShell(all.shell) ? all.shell : "auto",
      restoreLayout: all.restore_layout !== "0",
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
}));

/** The `shell` argument for pty_spawn: `undefined` lets Rust pick the default. */
export function shellForSpawn(choice: ShellChoice): string | undefined {
  return choice === "auto" ? undefined : `${choice}.exe`;
}
