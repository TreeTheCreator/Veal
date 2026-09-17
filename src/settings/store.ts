// App settings, written through to the `settings` table on every change.
// The pane layout lives in the same table but is owned by layout/persist.ts.
import { create } from "zustand";
import { db } from "../db";
import type { PaneKind } from "../layout/tree";

export type ShellChoice = "auto" | "pwsh" | "powershell" | "cmd" | "wsl";

export const SHELL_CHOICES: Array<{ value: ShellChoice; label: string }> = [
  { value: "auto", label: "Auto (pwsh, else powershell)" },
  { value: "pwsh", label: "pwsh" },
  { value: "powershell", label: "powershell" },
  { value: "cmd", label: "cmd" },
  { value: "wsl", label: "wsl" },
];

interface SettingsState {
  defaultKind: PaneKind;
  shell: ShellChoice;
  restoreLayout: boolean;
  /** Loads every key; returns the raw map so the caller can read `layout` too. */
  load(): Promise<Record<string, string>>;
  setDefaultKind(kind: PaneKind): void;
  setShell(shell: ShellChoice): void;
  setRestoreLayout(on: boolean): void;
}

const isKind = (v: string | undefined): v is PaneKind => v === "shell" || v === "agent";
const isShell = (v: string | undefined): v is ShellChoice =>
  SHELL_CHOICES.some((c) => c.value === v);

function persist(key: string, value: string) {
  db.settingsSet(key, value).catch((e) => console.warn("settings_set", key, e));
}

export const useSettings = create<SettingsState>((set) => ({
  defaultKind: "shell",
  shell: "auto",
  restoreLayout: true,

  async load() {
    const all = await db.settingsGetAll();
    set({
      defaultKind: isKind(all.default_kind) ? all.default_kind : "shell",
      shell: isShell(all.shell) ? all.shell : "auto",
      restoreLayout: all.restore_layout !== "0",
    });
    return all;
  },

  setDefaultKind(defaultKind) {
    set({ defaultKind });
    persist("default_kind", defaultKind);
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
