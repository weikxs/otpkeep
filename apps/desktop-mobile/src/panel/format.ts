/** Codes in groups of three (six digits), four (eight) or three and four (seven), for reading aloud. */
export function groupCode(code: string): string {
  switch (code.length) {
    case 6:
      return `${code.slice(0, 3)} ${code.slice(3)}`;
    case 7:
      return `${code.slice(0, 3)} ${code.slice(3)}`;
    case 8:
      return `${code.slice(0, 4)} ${code.slice(4)}`;
    default:
      return code;
  }
}

/** One or two characters standing for the service: the first letter of each of the first two words, or the
 * first character of a Chinese name. */
export function monogram(issuer: string, title: string): string {
  const source = (issuer || title).trim();
  if (source === "") return "#";
  const first = Array.from(source)[0] ?? "#";
  if (/[㐀-鿿]/u.test(first)) return first;
  const words = source.split(/[\s._@-]+/u).filter((w) => w !== "");
  const letters = words.slice(0, 2).map((w) => Array.from(w)[0] ?? "");
  return letters.join("").toUpperCase() || first.toUpperCase();
}

/** Eight calm tile colours; the name decides which, so a service keeps its colour. */
export const TILE_COLORS = 8;

export function tileColor(name: string): number {
  let hash = 2166136261;
  for (const char of name.toLowerCase()) {
    hash ^= char.codePointAt(0) ?? 0;
    hash = Math.imul(hash, 16777619) >>> 0;
  }
  return hash % TILE_COLORS;
}

/** The shortcut label for row `n` (1–10). */
export function shortcutLabel(n: number, isMac: boolean): string {
  const digit = n === 10 ? "0" : String(n);
  return isMac ? `⌃⌥⌘${digit}` : `Ctrl+Shift+Alt+${digit}`;
}
