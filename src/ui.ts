// UI state: which view is showing, and the toast. The view is written to the
// `settings` table so a restart — including the dev loop's reload when a
// file of the running app itself is saved — comes back to the same view.
import { create } from "zustand";
import { db } from "./db";

export type View = "terminal" | "files" | "tasks" | "settings";

export const TAB_VIEWS: View[] = ["terminal", "files", "tasks"];

export const TAB_LABEL: Record<View, string> = { terminal: "Terminal", files: "Files", tasks: "Tasks", settings: "Settings" };

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

export type TerminalKeyAction = "copy" | "paste" | "newline" | "view" | "pass";

/**
 * What Shift+Enter and Ctrl+Enter send: ESC CR, the bytes of Alt+Enter, which
 * agent CLIs such as Claude Code read as "new line" rather than "submit".
 * xterm alone sends a bare CR for both, so they submitted.
 */
export const NEWLINE_SEQUENCE = "\x1b\r";

/**
 * What Ctrl+V sends when the clipboard holds an image and no text: ESC v, the
 * bytes of Alt+V, Claude Code's image paste on Windows. The browser's paste
 * event carries no text then, so xterm would paste nothing.
 */
export const IMAGE_PASTE_SEQUENCE = "\x1bv";

/**
 * What a keydown over a terminal does before xterm sees it (plan-16):
 * Ctrl+C or Ctrl+Shift+C with a selection copies it (as Windows Terminal
 * does) instead of sending ^C; Ctrl+V or Ctrl+Shift+V pastes (xterm alone
 * would send ^V, which is how dictation apps' synthetic Ctrl+V got lost);
 * Shift+Enter or Ctrl+Enter sends NEWLINE_SEQUENCE; a view shortcut goes to
 * the window; the rest, including Ctrl+C with nothing selected, reaches the
 * shell.
 */
export function terminalKeyAction(e: KeyboardEvent, hasSelection: boolean): TerminalKeyAction {
  if (isViewShortcut(e)) return "view";
  if (hasSelection && e.ctrlKey && !e.altKey && !e.metaKey && e.key.toLowerCase() === "c") return "copy";
  if (e.ctrlKey && !e.altKey && !e.metaKey && e.key.toLowerCase() === "v") return "paste";
  if (e.key === "Enter" && (e.shiftKey || e.ctrlKey) && !e.altKey && !e.metaKey) return "newline";
  return "pass";
}

/** Mockup toast: 2200 ms, replaces any toast still showing. */
const TOAST_MS = 2200;

interface UiState {
  view: View;
  /** The tab Settings returns to: the last of Terminal / Files / Tasks shown. */
  lastTab: View;
  toast: string | null;
  setView(view: View): void;
  /** The sidebar's Settings button: opens Settings, or returns to `lastTab` if it is open. */
  toggleSettings(): void;
  showToast(message: string): void;
}

let toastTimer: ReturnType<typeof setTimeout> | undefined;

export const useUi = create<UiState>((set, get) => ({
  view: "terminal",
  lastTab: "terminal",
  toast: null,
  setView(view) {
    set(view === "settings" ? { view } : { view, lastTab: view });
    db.settingsSet(VIEW_KEY, view).catch((e) => console.warn("settings_set", VIEW_KEY, e));
  },
  toggleSettings() {
    const { view, lastTab, setView } = get();
    setView(view === "settings" ? lastTab : "settings");
  },
  showToast(toast) {
    set({ toast });
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => set({ toast: null }), TOAST_MS);
  },
}));
