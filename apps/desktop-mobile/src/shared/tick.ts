import { useEffect, useState } from "preact/hooks";

const SECOND_MS = 1000;
/** Ticks land this long after the second, so a timer firing a little early still sees the new second. */
const TICK_SLACK_MS = 5;

/** Milliseconds from `now` to the next whole second (a whole second when `now` is on one). */
export function msToNextSecond(now: number): number {
  return SECOND_MS - (((now % SECOND_MS) + SECOND_MS) % SECOND_MS);
}

/**
 * The local clock, read again just after each whole second of the core's clock (`offset` ms ahead of the local
 * one) while `active`. Countdowns show whole seconds, so ticking more often would only draw the same thing again.
 * Returns the time and a setter for when the window reads the clock itself.
 */
export function useSecondTick(clock: () => number, offset: number, active: boolean): [number, (now: number) => void] {
  const [now, setNow] = useState(clock);
  useEffect(() => {
    if (!active) return;
    let timer: ReturnType<typeof setTimeout>;
    const schedule = () => {
      timer = setTimeout(() => {
        setNow(clock());
        schedule();
      }, msToNextSecond(clock() + offset) + TICK_SLACK_MS);
    };
    schedule();
    return () => clearTimeout(timer);
  }, [clock, offset, active]);
  return [now, setNow];
}
