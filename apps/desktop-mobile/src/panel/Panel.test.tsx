import { fireEvent, render, screen, waitFor } from "@testing-library/preact";
import { describe, expect, it, vi } from "vitest";
import type { PanelApi, PanelState } from "./api";
import { demoApi } from "./demo";
import { strings } from "./i18n";
import { Panel } from "./Panel";

const zh = strings("zh-CN");
// 10 seconds into a 30-second step: no "next" code yet.
const STEADY = 1_700_000_020_000;
// 3 seconds before the step ends.
const ENDING = 1_700_000_037_000;

function show(api: PanelApi, at = STEADY) {
  return render(<Panel api={api} text={zh} isMac clock={() => at} />);
}

describe("Panel", () => {
  it("lists pinned entries first under their own heading", async () => {
    show(demoApi({ clock: () => STEADY }));

    const rows = await screen.findAllByTestId("entry");
    expect(rows).toHaveLength(6);
    expect(screen.getByText("置顶")).toBeTruthy();
    expect(screen.getByText("全部")).toBeTruthy();
    expect(rows[0]?.textContent).toContain("GitHub");
  });

  it("shows codes in groups of three", async () => {
    show(demoApi({ clock: () => STEADY }));

    const codes = await screen.findAllByTestId("code");
    expect(codes[0]?.textContent).toMatch(/^\d{3} \d{3}$/u);
  });

  it("shows the next code when the current one is ending", async () => {
    show(demoApi({ clock: () => ENDING }), ENDING);

    const next = await screen.findAllByText(/^下一个/u);
    expect(next.length).toBeGreaterThan(0);
  });

  it("copies on click and says when the clipboard clears", async () => {
    const api = demoApi({ clock: () => STEADY });
    show(api);

    const rows = await screen.findAllByTestId("entry");
    fireEvent.click(rows[1] as HTMLElement);

    expect(await screen.findByRole("status")).toHaveProperty("textContent", "已复制 · 30 秒后从剪贴板清除");
    expect(api.copied).toEqual(["d2"]);
  });

  it("moves the selection with arrows and copies with Enter", async () => {
    const api = demoApi({ clock: () => STEADY });
    show(api);
    const search = await screen.findByRole("searchbox");

    fireEvent.keyDown(search, { key: "ArrowDown" });
    fireEvent.keyDown(search, { key: "ArrowDown" });
    fireEvent.keyDown(search, { key: "Enter" });

    await waitFor(() => expect(api.copied).toEqual(["d3"]));
  });

  it("copies row n with ⌘n", async () => {
    const api = demoApi({ clock: () => STEADY });
    show(api);
    const search = await screen.findByRole("searchbox");

    fireEvent.keyDown(search, { key: "4", metaKey: true });

    await waitFor(() => expect(api.copied).toEqual(["d4"]));
  });

  it("generates a counter code only when asked", async () => {
    const api = demoApi({ clock: () => STEADY });
    show(api);

    const generate = await screen.findByText("生成");
    fireEvent.click(generate);

    await waitFor(() => expect(api.copied).toEqual(["d5"]));
    await waitFor(() => expect(screen.queryByText("生成")).toBeNull());
  });

  it("filters by search and drops the group headings", async () => {
    show(demoApi({ clock: () => STEADY }));
    const search = await screen.findByRole("searchbox");

    fireEvent.input(search, { target: { value: "vpn" } });

    await waitFor(() => expect(screen.getAllByTestId("entry")).toHaveLength(1));
    expect(screen.queryByText("置顶")).toBeNull();
  });

  it("says when nothing matches", async () => {
    show(demoApi({ clock: () => STEADY }));
    const search = await screen.findByRole("searchbox");

    fireEvent.input(search, { target: { value: "zzz" } });

    expect(await screen.findByText("没有匹配的条目")).toBeTruthy();
  });

  it("clears the search with Esc before hiding", async () => {
    const api = demoApi({ clock: () => STEADY });
    const hide = vi.spyOn(api, "hide");
    show(api);
    const search = await screen.findByRole("searchbox");
    fireEvent.input(search, { target: { value: "vpn" } });

    fireEvent.keyDown(search, { key: "Escape" });
    await waitFor(() => expect((search as HTMLInputElement).value).toBe(""));
    expect(hide).not.toHaveBeenCalled();
    fireEvent.keyDown(search, { key: "Escape" });

    expect(hide).toHaveBeenCalledOnce();
  });

  it("offers import and add when there are no entries", async () => {
    const api = demoApi({ clock: () => STEADY, empty: true });
    const openMain = vi.spyOn(api, "openMain");
    show(api);

    fireEvent.click(await screen.findByText("导入 GoldenPassport 数据"));

    expect(openMain).toHaveBeenCalledWith("import");
    expect(screen.getByText("添加账号")).toBeTruthy();
  });

  it("unlocks with the master password and rejects a wrong one", async () => {
    show(demoApi({ clock: () => STEADY, status: "locked" }));
    const password = await screen.findByLabelText("主口令");

    fireEvent.input(password, { target: { value: "nope" } });
    fireEvent.click(screen.getByText("解锁"));
    expect(await screen.findByText("主口令不正确")).toBeTruthy();

    fireEvent.input(password, { target: { value: "demo" } });
    fireEvent.click(screen.getByText("解锁"));
    expect(await screen.findAllByTestId("entry")).toHaveLength(6);
  });

  it("locks from the search bar", async () => {
    show(demoApi({ clock: () => STEADY }));

    fireEvent.click(await screen.findByLabelText("锁定"));

    expect(await screen.findByText("OTPKeep 已锁定")).toBeTruthy();
  });

  it("sends the person to the main window when the data cannot be opened", async () => {
    const api = demoApi({ clock: () => STEADY, status: "unavailable" });
    const openMain = vi.spyOn(api, "openMain");
    show(api);

    fireEvent.click(await screen.findByText("在主窗口中处理"));

    expect(screen.getByText("数据文件不见了，可以从备份恢复")).toBeTruthy();
    expect(openMain).toHaveBeenCalledWith();
  });

  it("shows the sync status or read-only in the footer", async () => {
    const api = demoApi({ clock: () => STEADY });
    const readOnly: PanelApi = {
      ...api,
      state: async (q): Promise<PanelState> => ({ ...(await api.state(q)), readOnly: true }),
    };
    show(readOnly);

    expect(await screen.findByText("只读：新版本 OTPKeep 正在使用这些数据")).toBeTruthy();
  });

  it("says when the device is in a sync group", async () => {
    const api = demoApi({ clock: () => STEADY });
    show({ ...api, state: async (q) => ({ ...(await api.state(q)), inGroup: true }) });

    expect(await screen.findByText("已加入同步组")).toBeTruthy();
  });

  it("reports a failed copy", async () => {
    const api = demoApi({ clock: () => STEADY });
    show({ ...api, copy: () => Promise.reject(new Error("no clipboard")) });

    fireEvent.click((await screen.findAllByTestId("entry"))[0] as HTMLElement);

    expect(await screen.findByRole("status")).toHaveProperty("textContent", "复制失败");
  });
});

describe("strings", () => {
  it("falls back to English", () => {
    expect(strings("en-US").lock).toBe("Lock");
    expect(strings("fr").copiedToast(0)).toBe("Copied");
  });
});
