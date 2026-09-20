// UI state: which view is showing, and the toast. The view is written to the
// `settings` table so a restart — including the dev loop's reload when a
// file of the running app itself is saved — comes back to the same view.
import { create } from "zustand";
import { db } from "./db";

export type View = "terminal" | "files" | "tasks" | "settings";

export const TAB_VIEWS: View[] = ["terminal", "files", "tasks"];

export const VIEW_KEY = "view";

export const isView = (v: string | undefined): v is View =>
  v === "terminal" || v === "files" || v === "tasks" || v === "settings";

/** Ctrl+1/2/3 → Terminal / Files / Tasks. Shared with xterm's key filter. */
export function isViewShortcut(e: KeyboardEvent): boolean {
  return e.ctrlKey && !e.altKey && !e.metaKey && e.key >= "1" && e.key <= String(TAB_VIEWS.length);
}

export function viewForShortcut(e: KeyboardEvent): View | null {
  return isViewShortcut(e) ? TAB_VIEWS[Number(e.key) - 1] : null;
}

export type TerminalKeyAction = "copy" | "view" | "pass";

/**
 * What a keydown over a terminal does before xterm sees it (plan-16):
 * Ctrl+C or Ctrl+Shift+C with a selection copies it (as Windows Terminal
 * does) instead of sending ^C; a view shortcut goes to the window; the
 * rest, including Ctrl+C with nothing selected, reaches the shell.
 */
export function terminalKeyAction(e: KeyboardEvent, hasSelection: boolean): TerminalKeyAction {
  if (isViewShortcut(e)) return "view";
  if (hasSelection && e.ctrlKey && !e.altKey && !e.metaKey && e.key.toLowerCase() === "c") return "copy";
  return "pass";
}

/** Mockup toast: 2200 ms, replaces any toast still showing. */
const TOAST_MS = 2200;

interface UiState {
  view: View;
  toast: string | null;
  setView(view: View): void;
  showToast(message: string): void;
}

let toastTimer: ReturnType<typeof setTimeout> | undefined;

export const useUi = create<UiState>((set) => ({
  view: "terminal",
  toast: null,
  setView(view) {
    set({ view });
    db.settingsSet(VIEW_KEY, view).catch((e) => console.warn("settings_set", VIEW_KEY, e));
  },
  showToast(toast) {
    set({ toast });
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => set({ toast: null }), TOAST_MS);
  },
}));
