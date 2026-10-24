import { describe, expect, it } from "vitest";
import { formatShortcut, recordShortcut } from "./shortcut";

const press = (code: string, mods: Partial<Record<"ctrlKey" | "altKey" | "shiftKey" | "metaKey", boolean>> = {}) => ({
  ctrlKey: false,
  altKey: false,
  shiftKey: false,
  metaKey: false,
  code,
  ...mods,
});

describe("formatShortcut", () => {
  it("writes Mac symbols in the system's order", () => {
    expect(formatShortcut("shift+control+alt+super+KeyG", true)).toBe("⌃⌥⇧⌘G");
    expect(formatShortcut("control+alt+super+Digit0", true)).toBe("⌃⌥⌘0");
  });

  it("spells keys out elsewhere", () => {
    expect(formatShortcut("control+alt+F5", false)).toBe("Ctrl+Alt+F5");
    expect(formatShortcut("alt+super+Space", false)).toBe("Alt+Win+Space");
  });

  it("names punctuation and arrows", () => {
    expect(formatShortcut("control+Slash", true)).toBe("⌃/");
    expect(formatShortcut("control+ArrowUp", false)).toBe("Ctrl+↑");
  });
});

describe("recordShortcut", () => {
  it("records a combination in the core's form", () => {
    expect(recordShortcut(press("KeyG", { ctrlKey: true, altKey: true, metaKey: true }))).toEqual({
      kind: "shortcut",
      shortcut: "Control+Alt+Super+KeyG",
    });
  });

  it("waits while only modifiers are held", () => {
    expect(recordShortcut(press("AltLeft", { altKey: true }))).toEqual({ kind: "incomplete" });
    expect(recordShortcut(press("MetaRight", { metaKey: true }))).toEqual({ kind: "incomplete" });
  });

  it("asks for a real modifier", () => {
    expect(recordShortcut(press("KeyG"))).toEqual({ kind: "needsModifier" });
    expect(recordShortcut(press("KeyG", { shiftKey: true }))).toEqual({ kind: "needsModifier" });
  });
});
