import { describe, expect, it } from "vitest";
import { type ListLayout, linesOf, offsetOf, visibleSpan } from "./windowing";

const ONE_COLUMN: ListLayout = { columns: 1, rowHeight: 54, headingHeight: 20, gap: 8 };
const rows = (n: number) => Array.from({ length: n }, () => "row" as const);

describe("linesOf", () => {
  it("puts rows side by side and a heading on a line of its own", () => {
    const lines = linesOf(["heading", "row", "row", "row", "heading", "row"], 2);

    expect(lines.map((l) => [l.first, l.end])).toEqual([
      [0, 1],
      [1, 3],
      [3, 4],
      [4, 5],
      [5, 6],
    ]);
  });
});

describe("visibleSpan", () => {
  it("draws only the rows on screen and a few around them", () => {
    // 200 rows, 62 px apart; the screen shows rows 100 to 108.
    const span = visibleSpan(rows(200), ONE_COLUMN, 6200, 500, 2);

    expect(span.first).toBe(98);
    expect(span.end).toBe(111);
    expect(span.before).toBe(98 * 62);
    expect(span.after).toBe((200 - 111) * 62);
  });

  it("starts at the top", () => {
    const span = visibleSpan(rows(200), ONE_COLUMN, 0, 500, 2);

    expect(span.first).toBe(0);
    expect(span.before).toBe(0);
  });

  it("keeps the list's full height", () => {
    const span = visibleSpan(rows(200), ONE_COLUMN, 3000, 500, 2);
    const drawn = span.end - span.first;

    expect(span.before + drawn * 54 + (drawn - 1) * 8 + span.after).toBe(200 * 54 + 199 * 8);
  });

  it("starts each drawn part at the beginning of a line of two", () => {
    const span = visibleSpan(rows(200), { ...ONE_COLUMN, columns: 2 }, 3100, 500, 1);

    expect(span.first % 2).toBe(0);
    expect(span.end % 2).toBe(0);
  });

  it("counts headings at their own height", () => {
    const items = ["heading", ...rows(3), "heading", ...rows(100)] as const;

    expect(offsetOf([...items], ONE_COLUMN, 5)).toBe(20 + 8 + 3 * 62 + 20 + 8);
  });
});
