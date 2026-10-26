import { useCallback, useEffect, useState } from "preact/hooks";
import { needsReload, useVisible } from "../shared/visible";
import type { PanelApi, PanelState } from "./api";

const TICK_MS = 250;

/**
 * The entries matching `query`, kept current: reloaded when the data changes elsewhere, when a code ends or
 * the clock jumps back, and ticking for the countdowns while the window is on screen. `coreNow` is the core's
 * clock.
 */
export function useLiveState(api: Pick<PanelApi, "state" | "onChange">, query: string, clock: () => number) {
  const [state, setState] = useState<PanelState | null>(null);
  const [offset, setOffset] = useState(0);
  const [now, setNow] = useState(clock());

  const load = useCallback(
    async (q: string) => {
      const next = await api.state(q);
      setOffset(next.nowMs - clock());
      setNow(clock());
      setState(next);
    },
    [api, clock],
  );
  const reload = useCallback(() => load(query), [load, query]);
  const visible = useVisible(() => void reload());

  useEffect(() => void load(query), [query, load]);
  // Names and shortcuts changed elsewhere, or the window opened again.
  useEffect(() => api.onChange(() => void load(query)), [api, load, query]);

  const coreNow = now + offset;
  const stale = visible && needsReload(state?.entries, coreNow);
  useEffect(() => {
    if (stale) void load(query);
  }, [stale, load, query]);

  useEffect(() => {
    if (!visible) return;
    const timer = setInterval(() => setNow(clock()), TICK_MS);
    return () => clearInterval(timer);
  }, [clock, visible]);

  return { state, setState, coreNow, reload };
}
