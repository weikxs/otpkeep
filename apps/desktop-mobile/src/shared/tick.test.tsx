import { act, render } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { msToNextSecond, useSecondTick } from "./tick";

// 250 ms past a whole second.
const START = 1_700_000_020_250;

function Probe({ offset, active, seen }: { offset: number; active: boolean; seen: number[] }) {
  const [now] = useSecondTick(Date.now, offset, active);
  seen.push(now);
  return null;
}

/** Lets `seconds` seconds pass, drawing after each one as a real window would. */
async function pass(seconds: number) {
  for (let i = 0; i < seconds; i += 1) {
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1000);
    });
  }
}

describe("msToNextSecond", () => {
  it("counts to the next whole second", () => {
    expect(msToNextSecond(1_000_250)).toBe(750);
    expect(msToNextSecond(1_000_999)).toBe(1);
  });

  it("waits a whole second when already on one", () => {
    expect(msToNextSecond(1_000_000)).toBe(1000);
  });
});

describe("useSecondTick", () => {
  beforeEach(() => vi.useFakeTimers({ now: START }));
  afterEach(() => vi.useRealTimers());

  it("ticks once a second, on the second", async () => {
    const seen: number[] = [];
    render(<Probe offset={0} active seen={seen} />);

    await pass(10);

    const ticks = seen.slice(1);
    expect(ticks).toHaveLength(10);
    for (const at of ticks) expect(at % 1000).toBeLessThan(50);
  });

  it("follows the seconds of the core's clock", async () => {
    const seen: number[] = [];
    // The core is 400 ms ahead, so its seconds begin at .600 on this clock.
    render(<Probe offset={400} active seen={seen} />);

    await pass(3);

    expect(seen.slice(1)).toHaveLength(3);
    for (const at of seen.slice(1)) expect((at + 400) % 1000).toBeLessThan(50);
  });

  it("does not tick while inactive", async () => {
    const seen: number[] = [];
    render(<Probe offset={0} active={false} seen={seen} />);

    await pass(10);

    expect(seen).toHaveLength(1);
  });
});
