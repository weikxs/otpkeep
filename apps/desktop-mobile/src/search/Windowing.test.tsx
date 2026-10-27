import { fireEvent, render, screen, waitFor } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { PanelEntry, PanelState } from "../panel/api";
import { strings } from "../panel/i18n";
import type { SearchApi } from "./api";
import { demoSearchApi } from "./demo";
import { searchStrings } from "./i18n";
import { SearchApp } from "./SearchApp";

const zh = strings("zh-CN");
// 10 seconds into a 30-second step.
const STEADY = 1_700_000_020_000;
const COUNT = 200;
// Rows and a list of a set height, as the stylesheets make them.
const ROW_PX = 40;
const VIEW_PX = 400;

function bigApi(): SearchApi {
  const api = demoSearchApi({ clock: () => STEADY });
  const entries: PanelEntry[] = Array.from({ length: COUNT }, (_, i) => ({
    id: `e${i + 1}`,
    title: `Demo ${i + 1}`,
    issuer: "Example Cloud",
    account: `demo${i + 1}@example.com`,
    note: "",
    tags: [],
    kind: "totp",
    pinned: false,
    code: "123456",
    periodS: 30,
    validUntilMs: STEADY + 10_000,
  }));
  const state: PanelState = { status: "ready", entries, nowMs: STEADY, usesMasterPassword: false, inGroup: false, readOnly: false };
  vi.spyOn(api, "state").mockResolvedValue(state);
  return api;
}

/** Gives the page the geometry a browser would; the test document has no layout of its own. */
function fakeLayout() {
  const height = (element: HTMLElement) => {
    if (element.classList.contains("entry")) return ROW_PX;
    if (element.classList.contains("entries")) return VIEW_PX;
    return 0;
  };
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockImplementation(function (this: HTMLElement) {
    return height(this);
  });
  vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockImplementation(function (this: HTMLElement) {
    return height(this);
  });
}

const drawn = () => screen.getAllByTestId("entry");

describe("a search panel with many accounts", () => {
  beforeEach(fakeLayout);
  afterEach(() => vi.restoreAllMocks());

  it("draws only the rows on screen, each with its place in the whole list", async () => {
    render(<SearchApp api={bigApi()} text={zh} words={searchStrings("zh-CN")} isMac clock={() => STEADY} />);

    await waitFor(() => expect(drawn().length).toBeLessThan(30));
    expect(drawn()[0]?.getAttribute("aria-setsize")).toBe(String(COUNT));
    expect(drawn()[0]?.getAttribute("aria-posinset")).toBe("1");
  });

  it("draws the row chosen with the arrow keys, however far down", async () => {
    render(<SearchApp api={bigApi()} text={zh} words={searchStrings("zh-CN")} isMac clock={() => STEADY} />);
    await waitFor(() => expect(drawn().length).toBeLessThan(30));
    const list = document.querySelector(".entries") as HTMLElement;
    let scrolled = 0;
    Object.defineProperty(list, "scrollTop", {
      configurable: true,
      get: () => scrolled,
      set: (value: number) => {
        scrolled = value;
      },
    });

    for (let i = 0; i < 60; i += 1) fireEvent.keyDown(screen.getByLabelText(zh.search), { key: "ArrowDown" });

    await waitFor(() => expect(screen.getByText("Demo 61").closest("li")?.getAttribute("aria-selected")).toBe("true"));
  });
});
