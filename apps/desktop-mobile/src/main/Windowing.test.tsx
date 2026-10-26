import { fireEvent, render, screen, waitFor, within } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Entry, MainApi, MainState } from "./api";
import { demoMain } from "./demo";
import { mainStrings } from "./i18n";
import { MainApp } from "./MainApp";

const zh = mainStrings("zh-CN");
// 10 seconds into a 30-second step.
const STEADY = 1_700_000_020_000;
const COUNT = 200;
// What the stylesheet makes of the list in a real window: 54 px rows, 8 px apart, a 500 px tall view.
const ROW_PX = 54;
const VIEW_PX = 500;

function many(): Entry[] {
  return Array.from({ length: COUNT }, (_, i) => ({
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
}

function bigApi(): MainApi {
  const api = demoMain({ clock: () => STEADY });
  const state: MainState = { status: "ready", entries: many(), nowMs: STEADY, usesMasterPassword: false, inGroup: false, readOnly: false };
  vi.spyOn(api, "state").mockImplementation(async (q: string) =>
    q === "" ? state : { ...state, entries: state.entries.filter((e) => e.title.includes(q)) },
  );
  return api;
}

/** Gives the page the geometry a browser would; the test document has no layout of its own. */
function fakeLayout() {
  const height = (element: HTMLElement) => {
    if (element.classList.contains("entry-row")) return ROW_PX;
    if (element.classList.contains("list")) return VIEW_PX;
    return 0;
  };
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockImplementation(function (this: HTMLElement) {
    return height(this);
  });
  vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockImplementation(function (this: HTMLElement) {
    return height(this);
  });
  const style = window.getComputedStyle;
  vi.spyOn(window, "getComputedStyle").mockImplementation((element: Element) => {
    const real = style(element);
    if (!element.classList.contains("entries-list")) return real;
    return { ...real, gridTemplateColumns: "640px", rowGap: "8px" } as CSSStyleDeclaration;
  });
}

function show(api: MainApi) {
  return render(<MainApp api={api} text={zh} isMac clock={() => STEADY} />);
}

describe("a main window with many accounts", () => {
  beforeEach(fakeLayout);
  afterEach(() => vi.restoreAllMocks());

  it("draws only the rows on screen and keeps the list's full height", async () => {
    show(bigApi());

    await waitFor(() => expect(screen.getAllByTestId("entry").length).toBeLessThan(30));
    const list = document.querySelector(".entries-list") as HTMLElement;
    expect(parseFloat(list.style.paddingBottom)).toBeGreaterThan(150 * ROW_PX);
  });

  it("tells screen readers each row's place in the whole list", async () => {
    show(bigApi());

    await waitFor(() => expect(screen.getAllByTestId("entry").length).toBeLessThan(30));
    const first = screen.getAllByTestId("entry")[0] as HTMLElement;
    expect(first.getAttribute("aria-setsize")).toBe(String(COUNT));
    expect(first.getAttribute("aria-posinset")).toBe("1");
  });

  it("draws the rows further down once scrolled there", async () => {
    show(bigApi());
    await waitFor(() => expect(screen.getAllByTestId("entry").length).toBeLessThan(30));
    const scroller = document.querySelector(".list") as HTMLElement;

    // The test document cannot scroll, so the scroll position is set the way a browser would report it: the
    // list moves up by as much as the view scrolled down.
    const scrolled = 100 * (ROW_PX + 8);
    const list = document.querySelector(".entries-list") as HTMLElement;
    Object.defineProperty(scroller, "scrollTop", { value: scrolled, configurable: true });
    vi.spyOn(list, "getBoundingClientRect").mockReturnValue({ top: -scrolled } as DOMRect);
    fireEvent.scroll(scroller);

    expect(await screen.findByText("Demo 101")).toBeTruthy();
    expect(screen.queryByText("Demo 1")).toBeNull();
  });

  it("draws every row while one is being edited, so the editor keeps its place", async () => {
    show(bigApi());
    await waitFor(() => expect(screen.getAllByTestId("entry").length).toBeLessThan(30));

    fireEvent.click(within(screen.getAllByTestId("entry")[0] as HTMLElement).getByText("编辑"));

    await waitFor(() => expect(screen.getAllByTestId("entry")).toHaveLength(COUNT));
  });
});
