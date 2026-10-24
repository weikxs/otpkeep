import { fireEvent, render, screen, waitFor } from "@testing-library/preact";
import { describe, expect, it, vi } from "vitest";
import { demoApi } from "../panel/demo";
import { strings } from "../panel/i18n";
import type { SearchApi } from "./api";
import { searchStrings } from "./i18n";
import { SearchApp } from "./SearchApp";

const STEADY = 1_700_000_020_000;

/** A demo search panel whose hiding, opening and showing can be watched. */
function demoSearch(status: "ready" | "needsPassword" = "ready") {
  const base = demoApi({ clock: () => STEADY, status });
  let shown = () => {};
  const api = {
    ...base,
    hide: vi.fn(async () => {}),
    openMain: vi.fn(async (_page?: string) => {}),
    onShown: (listener: () => void) => {
      shown = listener;
      return () => {};
    },
  } satisfies SearchApi & { copied: string[] };
  return { api, show: () => shown() };
}

function show(api: SearchApi, isMac = true) {
  render(<SearchApp api={api} text={strings("zh-CN")} words={searchStrings("zh-CN")} isMac={isMac} clock={() => STEADY} />);
}

const field = () => screen.findByRole("searchbox", { name: "搜索名称、账号、备注或标签" });

function press(key: string, extra: Partial<KeyboardEventInit> = {}) {
  fireEvent.keyDown(screen.getByRole("searchbox"), { key, ...extra });
}

describe("search panel", () => {
  it("finds an account as you type, then Enter copies it and closes", async () => {
    const { api } = demoSearch();
    show(api);

    fireEvent.input(await field(), { target: { value: "vpn" } });
    await waitFor(() => expect(screen.getAllByTestId("entry")).toHaveLength(1));
    press("Enter");

    await waitFor(() => expect(api.hide).toHaveBeenCalled());
    expect(api.copied).toEqual(["d4"]);
  });

  it("moves the choice with the arrow keys", async () => {
    const { api } = demoSearch();
    show(api);
    await field();
    await screen.findAllByTestId("entry");

    press("ArrowDown");
    press("ArrowDown");
    press("ArrowUp");
    press("Enter");

    await waitFor(() => expect(api.copied).toEqual(["d2"]));
  });

  it("generates an HOTP code before copying it", async () => {
    const { api } = demoSearch();
    const next = vi.spyOn(api, "nextHotp");
    show(api);

    fireEvent.input(await field(), { target: { value: "bank" } });
    await waitFor(() => expect(screen.getAllByTestId("entry")).toHaveLength(1));
    press("Enter");

    await waitFor(() => expect(api.copied).toEqual(["d5"]));
    expect(next).toHaveBeenCalledWith("d5");
  });

  it("opens the chosen account in the main window with ⌘E", async () => {
    const { api } = demoSearch();
    show(api);
    await field();
    await screen.findAllByTestId("entry");

    press("e", { metaKey: true });

    await waitFor(() => expect(api.openMain).toHaveBeenCalledWith("edit:d1"));
    expect(api.hide).toHaveBeenCalled();
  });

  it("uses Ctrl+E outside macOS", async () => {
    const { api } = demoSearch();
    show(api, false);
    await field();
    await screen.findAllByTestId("entry");

    press("e", { ctrlKey: true });

    await waitFor(() => expect(api.openMain).toHaveBeenCalledWith("edit:d1"));
    expect(screen.getByText("Ctrl+E")).toBeTruthy();
  });

  it("Esc clears the search first, then closes", async () => {
    const { api } = demoSearch();
    show(api);
    const input = await field();
    fireEvent.input(input, { target: { value: "git" } });

    press("Escape");
    await waitFor(() => expect((screen.getByRole("searchbox") as HTMLInputElement).value).toBe(""));
    expect(api.hide).not.toHaveBeenCalled();
    press("Escape");

    expect(api.hide).toHaveBeenCalled();
  });

  it("starts afresh each time it is shown", async () => {
    const { api, show: reopen } = demoSearch();
    show(api);
    fireEvent.input(await field(), { target: { value: "nothing like this" } });
    expect(await screen.findByText("没有匹配的条目")).toBeTruthy();

    reopen();

    await waitFor(() => expect(screen.getAllByTestId("entry")).toHaveLength(6));
  });

  it("stays open and says so when copying fails", async () => {
    const { api } = demoSearch();
    api.copy = vi.fn(async () => {
      throw new Error("clipboard busy");
    });
    show(api);
    await field();
    await screen.findAllByTestId("entry");

    press("Enter");

    expect(await screen.findByRole("alert")).toBeTruthy();
    expect(api.hide).not.toHaveBeenCalled();
  });

  it("asks for the master password when the data is not open yet", async () => {
    const { api } = demoSearch("needsPassword");
    show(api);

    expect(await screen.findByLabelText("主口令")).toBeTruthy();
    expect(screen.queryByRole("searchbox")).toBeNull();
  });
});
