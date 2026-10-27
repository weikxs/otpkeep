import { useCallback, useEffect, useState } from "preact/hooks";
import { useSecondTick } from "../shared/tick";
import { needsReload, useVisible } from "../shared/visible";
import type { PanelApi, PanelState } from "./api";

/**
 * The entries matching `query`, kept current: reloaded when the data changes elsewhere, when a code ends or
 * the clock jumps back, and ticking once a second for the countdowns while the window is on screen. `coreNow` is
 * the core's clock; `epoch` changes with each load, which starts the countdown rings again.
 */
export function useLiveState(api: Pick<PanelApi, "state" | "onChange">, query: string, clock: () => number) {
  const [state, setState] = useState<PanelState | null>(null);
  const [offset, setOffset] = useState(0);
  const [epoch, setEpoch] = useState(0);
  const load = useCallback(
    async (q: string) => {
      const next = await api.state(q);
      setOffset(next.nowMs - clock());
      setNow(clock());
      setEpoch((n) => n + 1);
      setState(next);
    },
    [api, clock],
  );
  const reload = useCallback(() => load(query), [load, query]);
  const visible = useVisible(() => void reload());
  const [now, setNow] = useSecondTick(clock, offset, visible && hasCountdown(state));

  useEffect(() => void load(query), [query, load]);
  // Names and shortcuts changed elsewhere, or the window opened again.
  useEffect(() => api.onChange(() => void load(query)), [api, load, query]);

  const coreNow = now + offset;
  const stale = visible && needsReload(state?.entries, coreNow);
  useEffect(() => {
    if (stale) void load(query);
  }, [stale, load, query]);

  return { state, setState, coreNow, epoch, reload };
}

/** Whether anything shown changes with time; otherwise the window does not tick at all. */
function hasCountdown(state: PanelState | null): boolean {
  return (state?.entries ?? []).some((e) => e.validUntilMs !== undefined);
}
