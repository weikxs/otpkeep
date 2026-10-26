import { describe, expect, it } from "vitest";
import { TILE_COLORS, detailsBeside, displayName, groupCode, monogram, tileColor } from "./format";

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

describe("displayName", () => {
  it("shows the service of an imported Issuer:account name", () => {
    expect(displayName({ title: "Example Cloud:ops@example.com", issuer: "", account: "ops@example.com" })).toBe("Example Cloud");
    expect(displayName({ title: "Example Cloud:demo-user@123456789012", issuer: "", account: "demo-user" })).toBe("Example Cloud");
  });

  it("shows the service when the name is just the account", () => {
    expect(displayName({ title: "123456789012", issuer: "Example Cloud", account: "123456789012" })).toBe("Example Cloud");
    expect(displayName({ title: "123456789012", issuer: "", account: "123456789012" })).toBe("123456789012");
  });

  it("keeps any other name", () => {
    expect(displayName({ title: "Demo Git:work", issuer: "", account: "demo@example.com" })).toBe("Demo Git:work");
    expect(displayName({ title: "示例银行", issuer: "示例银行", account: "" })).toBe("示例银行");
  });
});

describe("detailsBeside", () => {
  it("leaves out what the name already says", () => {
    expect(detailsBeside("Example Cloud", ["Example Cloud", "ops@example.com", ""])).toEqual(["ops@example.com"]);
    expect(detailsBeside("Example Cloud:ops@example.com", ["Example Cloud", "OPS@example.com", "note"])).toEqual(["note"]);
  });
});
