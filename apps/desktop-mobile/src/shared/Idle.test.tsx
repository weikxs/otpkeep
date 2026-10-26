import { act, render } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { demoApi } from "../panel/demo";
import { strings } from "../panel/i18n";
import { Panel } from "../panel/Panel";
import { demoMain } from "../main/demo";
import { mainStrings } from "../main/i18n";
import { MainApp } from "../main/MainApp";

// 10 seconds into a 30-second step.
const START = 1_700_000_020_000;
const TWO_MINUTES_MS = 120_000;

function setVisibility(state: "visible" | "hidden") {
  Object.defineProperty(document, "visibilityState", { value: state, configurable: true });
  document.dispatchEvent(new Event("visibilitychange"));
}

async function settle() {
  await act(async () => {
    await vi.advanceTimersByTimeAsync(0);
  });
}

async function pass(ms: number) {
  await act(async () => {
    await vi.advanceTimersByTimeAsync(ms);
  });
}

describe("windows at rest", () => {
  beforeEach(() => {
    vi.useFakeTimers({ now: START });
    setVisibility("visible");
  });
  afterEach(() => {
    setVisibility("visible");
    vi.useRealTimers();
  });

  it("the panel asks for nothing while hidden and catches up when shown", async () => {
    const api = demoApi({ clock: Date.now });
    const state = vi.spyOn(api, "state");
    render(<Panel api={api} text={strings("zh-CN")} isMac clock={Date.now} />);
    await settle();
    await pass(TWO_MINUTES_MS);
    expect(state.mock.calls.length).toBeGreaterThan(1);

    setVisibility("hidden");
    await settle();
    const before = state.mock.calls.length;
    await pass(TWO_MINUTES_MS);
    expect(state.mock.calls.length).toBe(before);

    setVisibility("visible");
    await settle();
    expect(state.mock.calls.length).toBe(before + 1);
  });

  it("the main window asks for nothing while hidden and catches up when shown", async () => {
    const api = demoMain({ clock: Date.now });
    const state = vi.spyOn(api, "state");
    render(<MainApp api={api} text={mainStrings("zh-CN")} isMac clock={Date.now} />);
    await settle();

    setVisibility("hidden");
    await settle();
    const before = state.mock.calls.length;
    await pass(TWO_MINUTES_MS);
    expect(state.mock.calls.length).toBe(before);

    setVisibility("visible");
    await settle();
    expect(state.mock.calls.length).toBe(before + 1);
  });

  it("the main window reloads when the clock went back by more than a period", async () => {
    let offsetMs = 0;
    const clock = () => Date.now() + offsetMs;
    const api = demoMain({ clock });
    const state = vi.spyOn(api, "state");
    render(<MainApp api={api} text={mainStrings("zh-CN")} isMac clock={clock} />);
    await settle();
    const before = state.mock.calls.length;

    offsetMs = -TWO_MINUTES_MS;
    // The window ticks just after each whole second.
    await pass(1100);

    expect(state.mock.calls.length).toBeGreaterThan(before);
  });
});
