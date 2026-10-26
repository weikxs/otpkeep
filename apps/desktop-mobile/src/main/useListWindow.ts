import { type RefObject } from "preact";
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "preact/hooks";
import { type ListItem, type ListLayout, type Span, offsetOf, visibleSpan } from "./windowing";

/** Lines drawn beyond each edge of the view, so Tab and the arrow keys always find the next row drawn. */
const OVERSCAN_LINES = 3;

export interface ListWindow {
  /** The scrolling element around the list. */
  scroller: RefObject<HTMLElement>;
  /** The list (`ul.entries-list`). */
  list: RefObject<HTMLUListElement>;
  /** What to draw; `null` draws everything (not measured yet, or windowing is off). */
  span: Span | null;
  onScroll: () => void;
  /** Scrolls item `index` into view, so it is drawn. */
  reveal: (index: number) => void;
}

/** Where the list's top is inside the scrolling element. */
function listTop(scroller: HTMLElement, list: HTMLElement): number {
  return list.getBoundingClientRect().top - scroller.getBoundingClientRect().top + scroller.scrollTop;
}

/** Reads the list's geometry from the rows drawn; `null` when there is none (no layout, or no row drawn). */
function measure(list: HTMLUListElement, previous: ListLayout | null): ListLayout | null {
  const row = list.querySelector<HTMLElement>(".entry-row:not(.editing)");
  const rowHeight = row?.offsetHeight ?? 0;
  if (rowHeight <= 0) return null;
  const style = window.getComputedStyle(list);
  const columns = style.gridTemplateColumns.split(" ").filter((c) => c !== "").length || 1;
  const gap = Number.parseFloat(style.rowGap) || 0;
  const heading = list.querySelector<HTMLElement>(".group-title");
  const margins = heading
    ? Number.parseFloat(window.getComputedStyle(heading).marginTop) + Number.parseFloat(window.getComputedStyle(heading).marginBottom)
    : 0;
  const headingHeight = heading ? heading.offsetHeight + (margins || 0) : (previous?.headingHeight ?? rowHeight);
  return { columns, rowHeight, headingHeight, gap };
}

const sameSpan = (a: Span | null, b: Span | null) => a?.first === b?.first && a?.end === b?.end && a?.before === b?.before;

/**
 * Draws only the part of a long list that is on screen (see windowing.ts). Off while `enabled` is false (a row
 * is being edited and has its own height) and wherever there is no layout to measure.
 */
export function useListWindow(items: readonly ListItem[], enabled: boolean): ListWindow {
  const scroller = useRef<HTMLElement>(null);
  const list = useRef<HTMLUListElement>(null);
  const [layout, setLayout] = useState<ListLayout | null>(null);
  const [view, setView] = useState({ top: 0, height: 0 });

  const spanAt = useCallback(
    (top: number, height: number, at: ListLayout | null) =>
      enabled && at !== null && height > 0 ? visibleSpan(items, at, top, height, OVERSCAN_LINES) : null,
    [items, enabled],
  );
  const span = spanAt(view.top, view.height, layout);
  const current = useRef({ span, layout, spanAt });
  current.current = { span, layout, spanAt };

  const readView = useCallback(() => {
    const [outer, inner] = [scroller.current, list.current];
    if (!outer || !inner) return null;
    return { top: outer.scrollTop - listTop(outer, inner), height: outer.clientHeight };
  }, []);

  const remeasure = useCallback(() => {
    const inner = list.current;
    const next = readView();
    if (!inner || !next) return;
    setLayout(measure(inner, current.current.layout));
    setView(next);
  }, [readView]);

  // Measured once rows are drawn, and again when the items change (a heading may have appeared).
  useLayoutEffect(() => {
    if (enabled) remeasure();
  }, [enabled, items, remeasure]);
  useEffect(() => {
    window.addEventListener("resize", remeasure);
    return () => window.removeEventListener("resize", remeasure);
  }, [remeasure]);

  const onScroll = useCallback(() => {
    const next = readView();
    if (!next) return;
    const { span: shown, layout: at, spanAt: spanOf } = current.current;
    // Drawn again only when other rows come into range, not on every scrolled pixel.
    if (!sameSpan(shown, spanOf(next.top, next.height, at))) setView(next);
  }, [readView]);

  const reveal = useCallback(
    (index: number) => {
      const [outer, inner, at] = [scroller.current, list.current, current.current.layout];
      if (!outer || !inner || !at) return;
      outer.scrollTop = listTop(outer, inner) + offsetOf(items, at, index);
      onScroll();
    },
    [items, onScroll],
  );

  return { scroller, list, span, onScroll, reveal };
}
