import { describe, expect, it } from "vitest";
import { terminalKeyAction, viewForShortcut } from "./ui";

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

  it("view shortcuts win, and agree with viewForShortcut", () => {
    const e = key({ key: "2", ctrlKey: true });
    expect(terminalKeyAction(e, true)).toBe("view");
    expect(viewForShortcut(e)).toBe("files");
    expect(terminalKeyAction(key({ key: "9", ctrlKey: true }), false)).toBe("pass");
  });
});
