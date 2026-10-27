import { act, fireEvent, render, screen, waitFor } from "@testing-library/preact";
import { describe, expect, it, vi } from "vitest";
import { strings } from "../panel/i18n";
import type { SearchApi } from "./api";
import { demoSearchApi } from "./demo";
import { searchStrings } from "./i18n";
import { SearchApp } from "./SearchApp";

const STEADY = 1_700_000_020_000;

/** A demo search panel whose hiding, opening and showing can be watched. */
function demoSearch(status: "ready" | "needsPassword" = "ready") {
  const base = demoSearchApi({ clock: () => STEADY, status });
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

describe("typing a code with ⇧Enter", () => {
  it("says so when nothing could be typed and the code was copied instead", async () => {
    const { api } = demoSearch();
    let failed: (result: { typed: boolean; fallback?: "FAILED"; clearsInS: number }) => void = () => {};
    show({
      ...api,
      onTypingFailed: (listener) => {
        failed = listener;
        return () => {};
      },
    });
    await screen.findAllByTestId("entry");

    act(() => failed({ typed: false, fallback: "FAILED", clearsInS: 30 }));

    expect((await screen.findByRole("status")).textContent).toBe("无法输入验证码，已改为复制（30 秒后清除）");
  });

  it("waits for Shift to be let go, then types and leaves hiding to the core", async () => {
    const { api } = demoSearch();
    await api.askTypingPermission();
    const typeCode = vi.spyOn(api, "typeCode");
    show(api);
    await field();
    await screen.findAllByTestId("entry");

    press("Enter", { shiftKey: true });
    await new Promise((r) => setTimeout(r, 20));
    expect(typeCode).not.toHaveBeenCalled();
    fireEvent.keyUp(window, { key: "Shift", shiftKey: false });

    await waitFor(() => expect(typeCode).toHaveBeenCalledWith("d1"));
    expect(api.copied).toEqual([]);
    expect(api.hide).not.toHaveBeenCalled();
  });

  it("copies instead without permission, says so and offers to ask for it", async () => {
    const { api } = demoSearch();
    const ask = vi.spyOn(api, "askTypingPermission");
    show(api);
    await field();
    await screen.findAllByTestId("entry");

    press("Enter", { shiftKey: true });
    fireEvent.keyUp(window, { key: "Shift", shiftKey: false });

    expect(await screen.findByText(/没有"辅助功能"授权，已改为复制（30 秒后清除）/u)).toBeTruthy();
    expect(api.copied).toEqual(["d1"]);
    fireEvent.click(screen.getByText("去授权"));
    await waitFor(() => expect(ask).toHaveBeenCalled());
    await waitFor(() => expect(screen.queryByText("去授权")).toBeNull());
  });

  it("types after a second even if Shift is never seen let go", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    try {
      const { api } = demoSearch();
      const typeCode = vi.spyOn(api, "typeCode");
      show(api);
      await field();
      await screen.findAllByTestId("entry");

      press("Enter", { shiftKey: true });
      await vi.advanceTimersByTimeAsync(1000);

      await waitFor(() => expect(typeCode).toHaveBeenCalledWith("d1"));
    } finally {
      vi.useRealTimers();
    }
  });
});

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

describe("language menu in the search panel", () => {
  it("sits in the footer and reports a choice", async () => {
    const { api } = demoSearch();
    const onChange = vi.fn();
    render(
      <SearchApp
        api={api}
        text={strings("zh-CN")}
        words={searchStrings("zh-CN")}
        isMac
        clock={() => STEADY}
        language={{ value: "zh", onChange }}
      />,
    );
    const menu = await screen.findByRole("combobox", { name: "语言" });

    expect((menu as HTMLSelectElement).value).toBe("zh");
    fireEvent.change(menu, { target: { value: "system" } });
    expect(onChange).toHaveBeenCalledWith("system");
  });
});
