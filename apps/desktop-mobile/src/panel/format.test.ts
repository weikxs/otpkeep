import { describe, expect, it } from "vitest";
import { TILE_COLORS, groupCode, monogram, shortcutLabel, tileColor } from "./format";

describe("groupCode", () => {
  it("groups six digits in threes", () => {
    expect(groupCode("438329")).toBe("438 329");
  });

  it("groups eight digits in fours", () => {
    expect(groupCode("43832958")).toBe("4383 2958");
  });

  it("splits seven digits three and four", () => {
    expect(groupCode("4383295")).toBe("438 3295");
  });

  it("leaves other lengths alone", () => {
    expect(groupCode("12345")).toBe("12345");
  });
});

describe("monogram", () => {
  it("takes the first letters of two words", () => {
    expect(monogram("Example Cloud", "x")).toBe("EC");
  });

  it("takes the first Chinese character", () => {
    expect(monogram("示例邮箱", "邮箱")).toBe("示");
  });

  it("falls back to the title, then to #", () => {
    expect(monogram("", "github")).toBe("G");
    expect(monogram("  ", "")).toBe("#");
  });
});

describe("tileColor", () => {
  it("keeps a name's colour and ignores case", () => {
    expect(tileColor("GitHub")).toBe(tileColor("github"));
  });

  it("stays within the palette", () => {
    for (const name of ["a", "b", "示例", "Example Bank", ""]) {
      const color = tileColor(name);
      expect(color).toBeGreaterThanOrEqual(0);
      expect(color).toBeLessThan(TILE_COLORS);
    }
  });
});

describe("shortcutLabel", () => {
  it("uses Mac symbols and 0 for the tenth row", () => {
    expect(shortcutLabel(1, true)).toBe("⌃⌥⌘1");
    expect(shortcutLabel(10, true)).toBe("⌃⌥⌘0");
  });

  it("spells out keys elsewhere", () => {
    expect(shortcutLabel(3, false)).toBe("Ctrl+Shift+Alt+3");
  });
});
