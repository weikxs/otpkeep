/**
 * Global shortcuts in the core's canonical form (`shift+control+alt+super+KeyG`), shown the way each system
 * writes them, and recorded from keyboard events in the same form.
 */

const MODIFIERS = ["control", "alt", "shift", "super"] as const;
type Modifier = (typeof MODIFIERS)[number];

const MAC_SYMBOLS: Record<Modifier, string> = { control: "⌃", alt: "⌥", shift: "⇧", super: "⌘" };
const OTHER_NAMES: Record<Modifier, string> = { control: "Ctrl", alt: "Alt", shift: "Shift", super: "Win" };
const KEY_NAMES: Record<string, string> = {
  Space: "Space",
  ArrowUp: "↑",
  ArrowDown: "↓",
  ArrowLeft: "←",
  ArrowRight: "→",
  Minus: "-",
  Equal: "=",
  BracketLeft: "[",
  BracketRight: "]",
  Backslash: "\\",
  Semicolon: ";",
  Quote: "'",
  Comma: ",",
  Period: ".",
  Slash: "/",
  Backquote: "`",
};

function keyName(code: string): string {
  if (code.startsWith("Key") && code.length === 4) return code.slice(3);
  if (code.startsWith("Digit") && code.length === 6) return code.slice(5);
  return KEY_NAMES[code] ?? code;
}

/** `control+alt+super+KeyG` → `⌃⌥⌘G` on a Mac, `Ctrl+Alt+Win+G` elsewhere. */
export function formatShortcut(canonical: string, isMac: boolean): string {
  const parts = canonical.split("+");
  const key = parts[parts.length - 1] ?? "";
  const mods = MODIFIERS.filter((m) => parts.slice(0, -1).includes(m));
  if (isMac) return `${mods.map((m) => MAC_SYMBOLS[m]).join("")}${keyName(key)}`;
  return [...mods.map((m) => OTHER_NAMES[m]), keyName(key)].join("+");
}

export interface KeyPress {
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
  metaKey: boolean;
  code: string;
}

/** Keys that only modify; a shortcut needs one more key. */
const MODIFIER_CODES = /^(Control|Alt|Shift|Meta|OS)(Left|Right)?$/u;

export type Recorded = { kind: "shortcut"; shortcut: string } | { kind: "incomplete" } | { kind: "needsModifier" };

/**
 * Turns a key press into the form the core accepts (`Control+Alt+Super+KeyG`). Shift alone is not enough:
 * the shortcut would fire while typing.
 */
export function recordShortcut(press: KeyPress): Recorded {
  if (MODIFIER_CODES.test(press.code) || press.code === "") return { kind: "incomplete" };
  if (!press.ctrlKey && !press.altKey && !press.metaKey) return { kind: "needsModifier" };
  const parts = [
    press.ctrlKey ? "Control" : "",
    press.altKey ? "Alt" : "",
    press.shiftKey ? "Shift" : "",
    press.metaKey ? "Super" : "",
    press.code,
  ].filter((p) => p !== "");
  return { kind: "shortcut", shortcut: parts.join("+") };
}

export function isMacPlatform(): boolean {
  return typeof navigator !== "undefined" && navigator.userAgent.includes("Mac");
}
