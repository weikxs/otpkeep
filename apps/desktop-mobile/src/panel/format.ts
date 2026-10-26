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

/** What naming an account needs. */
export interface Named {
  title: string;
  issuer: string;
  account: string;
}

const collapse = (text: string) => text.split(/\s+/u).filter((part) => part !== "").join(" ");

/**
 * The name to show (display only; the stored name is unchanged): the part before ":" of an imported name like
 * "Issuer:account", the issuer when the name is just the account, otherwise the name.
 */
export function displayName({ title, issuer, account }: Named): string {
  const name = collapse(title);
  const who = collapse(account).toLowerCase();
  if (who === "") return name;
  const colon = name.indexOf(":");
  if (colon > 0) {
    const head = name.slice(0, colon).trim();
    if (head !== "" && name.slice(colon + 1).trim().toLowerCase().includes(who)) return head;
  }
  const service = collapse(issuer);
  return service !== "" && name.toLowerCase() === who ? service : name;
}

/** The parts worth showing under `shown`: not empty and not already part of it (ignoring case). */
export function detailsBeside(shown: string, parts: readonly string[]): string[] {
  const name = shown.toLowerCase();
  return parts
    .map(collapse)
    .filter((part, index, all) => part !== "" && !name.includes(part.toLowerCase()) && all.indexOf(part) === index);
}
