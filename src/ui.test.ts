import { describe, expect, it } from "vitest";
import { terminalKeyAction, useUi, viewForShortcut } from "./ui";

const key = (init: Partial<KeyboardEvent> & { key: string }) =>
  ({ type: "keydown", ctrlKey: false, altKey: false, metaKey: false, shiftKey: false, ...init }) as KeyboardEvent;

describe("terminalKeyAction (plan-16)", () => {
  it("Ctrl+C with a selection copies; without one it reaches the shell", () => {
    expect(terminalKeyAction(key({ key: "c", ctrlKey: true }), true)).toBe("copy");
    expect(terminalKeyAction(key({ key: "C", ctrlKey: true, shiftKey: true }), true)).toBe("copy");
    expect(terminalKeyAction(key({ key: "c", ctrlKey: true }), false)).toBe("pass");
    expect(terminalKeyAction(key({ key: "C", ctrlKey: true, shiftKey: true }), false)).toBe("pass");
  });

  it("other modifiers and plain keys pass through, selection or not", () => {
    expect(terminalKeyAction(key({ key: "c" }), true)).toBe("pass");
    expect(terminalKeyAction(key({ key: "c", ctrlKey: true, altKey: true }), true)).toBe("pass");
    expect(terminalKeyAction(key({ key: "c", metaKey: true }), true)).toBe("pass");
    expect(terminalKeyAction(key({ key: "v", ctrlKey: true, altKey: true }), true)).toBe("pass");
    expect(terminalKeyAction(key({ key: "v" }), false)).toBe("pass");
  });

  it("Ctrl+V and Ctrl+Shift+V paste, selection or not", () => {
    expect(terminalKeyAction(key({ key: "v", ctrlKey: true }), false)).toBe("paste");
    expect(terminalKeyAction(key({ key: "v", ctrlKey: true }), true)).toBe("paste");
    expect(terminalKeyAction(key({ key: "V", ctrlKey: true, shiftKey: true }), false)).toBe("paste");
  });

  it("Shift+Enter and Ctrl+Enter send a new line; plain and Alt+Enter pass", () => {
    expect(terminalKeyAction(key({ key: "Enter", shiftKey: true }), false)).toBe("newline");
    expect(terminalKeyAction(key({ key: "Enter", ctrlKey: true }), true)).toBe("newline");
    expect(terminalKeyAction(key({ key: "Enter" }), false)).toBe("pass");
    expect(terminalKeyAction(key({ key: "Enter", altKey: true }), false)).toBe("pass");
  });

  it("view shortcuts win, and agree with viewForShortcut", () => {
    const e = key({ key: "2", ctrlKey: true });
    expect(terminalKeyAction(e, true)).toBe("view");
    expect(viewForShortcut(e)).toBe("files");
    expect(terminalKeyAction(key({ key: "9", ctrlKey: true }), false)).toBe("pass");
  });
});

describe("Settings toggle", () => {
  it("returns to the last tab shown, whether toggled or sent back", () => {
    useUi.getState().setView("files");
    useUi.getState().toggleSettings();
    expect(useUi.getState().view).toBe("settings");
    expect(useUi.getState().lastTab).toBe("files");
    useUi.getState().toggleSettings();
    expect(useUi.getState().view).toBe("files");

    useUi.getState().setView("tasks");
    useUi.getState().setView("settings");
    useUi.getState().setView(useUi.getState().lastTab);
    expect(useUi.getState().view).toBe("tasks");
  });
});
