import { useMemo } from "preact/hooks";

const RING_RADIUS = 7;

export interface RingProps {
  remaining: number;
  period: number;
  ending: boolean;
  label: string;
  /** When the code shown ends, in the core's clock: the ring is empty then. */
  validUntilMs: number;
  /** The core's clock now; read only when the ring starts its animation. */
  coreNow: number;
  /** Changes each time the window loads its codes again, which starts the animation again from the clock. */
  epoch: number;
}

/**
 * The countdown ring beside a time-based code; amber in the last seconds (design/12 §2). One CSS animation per
 * code empties it (ring.css), started part-way through with a negative delay, so the window does not redraw it
 * from script; a new code or a reload makes a new circle, which starts the animation again.
 */
export function Ring({ remaining, period, ending, label, validUntilMs, coreNow, epoch }: RingProps) {
  const periodMs = period * 1000;
  // Read when the circle is made, not on every tick: the running animation keeps its own time.
  const style = useMemo(() => {
    const elapsedMs = Math.round(Math.min(periodMs, Math.max(0, periodMs - (validUntilMs - coreNow))));
    return { "--ring-period": `${period}s`, "--ring-steps": `${period}`, "--ring-delay": `${-elapsedMs}ms` };
  }, [validUntilMs, period, periodMs, epoch]);
  return (
    <span class={`ring${ending ? " ending" : ""}`} title={label} aria-label={label}>
      <svg viewBox="0 0 18 18" width="18" height="18" aria-hidden="true">
        <circle class="ring-track" cx="9" cy="9" r={RING_RADIUS} />
        <circle
          key={`${validUntilMs}:${epoch}`}
          class="ring-value"
          cx="9"
          cy="9"
          r={RING_RADIUS}
          style={style}
          transform="rotate(-90 9 9)"
        />
      </svg>
      <span class="ring-seconds">{remaining}</span>
    </span>
  );
}
