import { useEffect, useRef, useState } from "preact/hooks";

/** Whether the window is on screen; outside a browser it counts as visible. */
export function isVisible(): boolean {
  return typeof document === "undefined" || document.visibilityState !== "hidden";
}

/**
 * Whether the window is on screen, following the page's visibility. Hidden windows stop their timers, so the
 * app costs nothing at rest; `onShown` runs each time the window comes back.
 */
export function useVisible(onShown?: () => void): boolean {
  const [visible, setVisible] = useState(isVisible);
  const shown = useRef(onShown);
  shown.current = onShown;
  useEffect(() => {
    const changed = () => {
      const now = isVisible();
      setVisible(now);
      if (now) shown.current?.();
    };
    document.addEventListener("visibilitychange", changed);
    return () => document.removeEventListener("visibilitychange", changed);
  }, []);
  return visible;
}

/**
 * Whether codes shown with these deadlines need asking for again at `coreNow`: one has ended, or the clock went
 * back so far that a deadline is more than a period away.
 */
export function needsReload(
  entries: readonly { validUntilMs?: number; periodS?: number }[] | undefined,
  coreNow: number,
): boolean {
  return (entries ?? []).some(
    (e) =>
      e.validUntilMs !== undefined &&
      (e.validUntilMs <= coreNow || (e.periodS !== undefined && e.validUntilMs - coreNow > e.periodS * 1000)),
  );
}
