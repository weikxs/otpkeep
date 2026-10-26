/**
 * Which items of a long list to draw: only the lines on screen and a few around them, with the rest stood in
 * for by padding of their height, so a window with hundreds of accounts draws a dozen rows (K3). Every row has
 * the same height (the list's CSS makes sure) and so does every heading; rows fill `columns` per line.
 */

/** What a list item is: a group heading on a line of its own, or a row. */
export type ListItem = "heading" | "row";

/** The list's measured geometry, in CSS pixels. */
export interface ListLayout {
  columns: number;
  rowHeight: number;
  headingHeight: number;
  /** Space between lines. */
  gap: number;
}

/** One line of the grid: items `first` up to (not including) `end`. */
export interface Line {
  first: number;
  end: number;
  heading: boolean;
}

/** The items to draw (`first` up to `end`) and the padding standing in for the lines before and after. */
export interface Span {
  first: number;
  end: number;
  before: number;
  after: number;
}

/** The grid's lines: a heading takes a line to itself, rows fill `columns` per line. */
export function linesOf(items: readonly ListItem[], columns: number): Line[] {
  const lines: Line[] = [];
  let index = 0;
  while (index < items.length) {
    if (items[index] === "heading") {
      lines.push({ first: index, end: index + 1, heading: true });
      index += 1;
      continue;
    }
    let end = index + 1;
    while (end < items.length && end - index < columns && items[end] === "row") end += 1;
    lines.push({ first: index, end, heading: false });
    index = end;
  }
  return lines;
}

/** Each line's top, from the list's top, and the list's whole height. */
function tops(lines: readonly Line[], layout: ListLayout): { starts: number[]; heights: number[]; total: number } {
  const starts: number[] = [];
  const heights: number[] = [];
  let y = 0;
  for (const line of lines) {
    const height = line.heading ? layout.headingHeight : layout.rowHeight;
    starts.push(y);
    heights.push(height);
    y += height + layout.gap;
  }
  return { starts, heights, total: lines.length === 0 ? 0 : y - layout.gap };
}

/**
 * The span to draw when the list's visible part runs from `top` (from the list's own top) for `height` pixels,
 * with `overscan` more lines either side, so moving focus with Tab always finds the next row drawn.
 */
export function visibleSpan(items: readonly ListItem[], layout: ListLayout, top: number, height: number, overscan: number): Span {
  const lines = linesOf(items, Math.max(1, layout.columns));
  if (lines.length === 0) return { first: 0, end: 0, before: 0, after: 0 };
  const { starts, heights, total } = tops(lines, layout);
  const bottom = top + height;
  let firstLine = lines.findIndex((_, i) => (starts[i] ?? 0) + (heights[i] ?? 0) >= top);
  if (firstLine < 0) firstLine = lines.length - 1;
  let lastLine = firstLine;
  while (lastLine + 1 < lines.length && (starts[lastLine + 1] ?? 0) <= bottom) lastLine += 1;
  firstLine = Math.max(0, firstLine - overscan);
  lastLine = Math.min(lines.length - 1, lastLine + overscan);
  const lastEnd = (starts[lastLine] ?? 0) + (heights[lastLine] ?? 0);
  return {
    first: lines[firstLine]?.first ?? 0,
    end: lines[lastLine]?.end ?? items.length,
    before: starts[firstLine] ?? 0,
    after: Math.max(0, total - lastEnd),
  };
}

/** How far down the list item `index` starts, to scroll it into view before it is drawn. */
export function offsetOf(items: readonly ListItem[], layout: ListLayout, index: number): number {
  const lines = linesOf(items, Math.max(1, layout.columns));
  const { starts } = tops(lines, layout);
  const line = lines.findIndex((l) => index >= l.first && index < l.end);
  return line < 0 ? 0 : (starts[line] ?? 0);
}
