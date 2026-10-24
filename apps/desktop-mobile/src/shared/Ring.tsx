const RING_RADIUS = 7;
const RING_LENGTH = 2 * Math.PI * RING_RADIUS;

export interface RingProps {
  remaining: number;
  period: number;
  ending: boolean;
  label: string;
}

/** The countdown ring beside a time-based code; amber in the last seconds (design/12 §2). */
export function Ring({ remaining, period, ending, label }: RingProps) {
  const fraction = Math.min(1, remaining / period);
  return (
    <span class={`ring${ending ? " ending" : ""}`} title={label} aria-label={label}>
      <svg viewBox="0 0 18 18" width="18" height="18" aria-hidden="true">
        <circle class="ring-track" cx="9" cy="9" r={RING_RADIUS} />
        <circle
          class="ring-value"
          cx="9"
          cy="9"
          r={RING_RADIUS}
          stroke-dasharray={`${RING_LENGTH * fraction} ${RING_LENGTH}`}
          transform="rotate(-90 9 9)"
        />
      </svg>
      <span class="ring-seconds">{remaining}</span>
    </span>
  );
}
