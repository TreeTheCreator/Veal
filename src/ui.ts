// Transient UI state: which view is showing, and the toast. Not persisted —
// the app always opens on Terminal.
import { create } from "zustand";

export type View = "terminal" | "files" | "tasks" | "settings";

export const TAB_VIEWS: View[] = ["terminal", "files", "tasks"];

/** Ctrl+1/2/3 → Terminal / Files / Tasks. Shared with xterm's key filter. */
export function isViewShortcut(e: KeyboardEvent): boolean {
  return e.ctrlKey && !e.altKey && !e.metaKey && e.key >= "1" && e.key <= String(TAB_VIEWS.length);
}

export function viewForShortcut(e: KeyboardEvent): View | null {
  return isViewShortcut(e) ? TAB_VIEWS[Number(e.key) - 1] : null;
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
  },
  showToast(toast) {
    set({ toast });
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => set({ toast: null }), TOAST_MS);
  },
}));
