import { fireEvent, render, screen, waitFor } from "@testing-library/preact";
import { describe, expect, it, vi } from "vitest";
import type { PanelApi, PanelState } from "./api";
import { demoApi } from "./demo";
import { strings } from "./i18n";
import { Panel, samePanelRow } from "./Panel";

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

  it("shows each row's shortcut the way the Mac writes it", async () => {
    show(demoApi({ clock: () => STEADY }));

    const rows = await screen.findAllByTestId("entry");
    expect(rows[0]?.textContent).toContain("⌃⌥⌘1");
  });

  it("opens the management page in the browser", async () => {
    const api = demoApi({ clock: () => STEADY });
    const openGate = vi.spyOn(api, "openGate");
    show(api);

    fireEvent.click(await screen.findByLabelText("在浏览器中管理"));

    expect(openGate).toHaveBeenCalledOnce();
  });

  it("reloads when the data changes elsewhere", async () => {
    const api = demoApi({ clock: () => STEADY });
    let changed = () => {};
    const state = vi.spyOn(api, "state");
    show({ ...api, onChange: (listener) => ((changed = listener), () => {}) });
    await screen.findAllByTestId("entry");
    const before = state.mock.calls.length;

    changed();

    await waitFor(() => expect(state.mock.calls.length).toBeGreaterThan(before));
  });

  it("has no lock button (ADR-019)", async () => {
    show(demoApi({ clock: () => STEADY }));

    await screen.findAllByTestId("entry");
    expect(screen.queryByLabelText("锁定")).toBeNull();
  });

  it("asks for the master password once and rejects a wrong one", async () => {
    show(demoApi({ clock: () => STEADY, status: "needsPassword" }));
    const password = await screen.findByLabelText("主口令");

    fireEvent.input(password, { target: { value: "nope" } });
    fireEvent.click(screen.getByText("打开"));
    expect(await screen.findByText("主口令不正确")).toBeTruthy();

    fireEvent.input(password, { target: { value: "demo" } });
    fireEvent.click(screen.getByText("打开"));
    expect(await screen.findAllByTestId("entry")).toHaveLength(6);
  });

  it("sends the person to the main window when the data cannot be opened", async () => {
    const api = demoApi({ clock: () => STEADY, status: "unavailable" });
    const openMain = vi.spyOn(api, "openMain");
    show(api);

    fireEvent.click(await screen.findByText("在主窗口中处理"));

    expect(screen.getByText("数据文件不见了。如果移动过它，请放回原处后点“重试”")).toBeTruthy();
    expect(openMain).toHaveBeenCalledWith();
  });

  it("tries to open the data again once another OTPKeep let it go", async () => {
    const api = demoApi({ clock: () => STEADY, status: "unavailable", problem: "BUSY" });
    const retry = vi.spyOn(api, "retryOpen");
    show(api);

    fireEvent.click(await screen.findByRole("button", { name: "重试" }));

    expect(await screen.findAllByTestId("entry")).toHaveLength(6);
    expect(retry).toHaveBeenCalledTimes(1);
  });

  it("offers no retry for a problem that trying again cannot fix", async () => {
    show(demoApi({ clock: () => STEADY, status: "unavailable", problem: "KEY_MISSING" }));

    expect(await screen.findByText("本机找不到这份数据的密钥")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "重试" })).toBeNull();
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
    expect(strings("en-US").unlock).toBe("Open");
    expect(strings("fr").copiedToast(0)).toBe("Copied");
  });
});

describe("copying in a browser preview", () => {
  it("really puts the demo code on the clipboard", async () => {
    show(demoApi({ clock: () => STEADY }));
    const code = (await screen.findAllByTestId("code"))[0]?.textContent?.replace(" ", "");

    fireEvent.click((await screen.findAllByTestId("entry"))[0] as HTMLElement);

    await waitFor(async () => expect(await navigator.clipboard.readText()).toBe(code));
  });

  it("says it failed instead of pretending, when the browser refuses", async () => {
    const write = vi.spyOn(navigator.clipboard, "writeText").mockRejectedValueOnce(new Error("denied"));
    show(demoApi({ clock: () => STEADY }));

    fireEvent.click((await screen.findAllByTestId("entry"))[0] as HTMLElement);

    expect(await screen.findByText("复制失败")).toBeTruthy();
    expect(screen.queryByText("已复制 ✓")).toBeNull();
    write.mockRestore();
  });
});

describe("language and the master password in the panel", () => {
  it("offers the language menu in the footer and reports a choice", async () => {
    const onChange = vi.fn();
    render(<Panel api={demoApi({ clock: () => STEADY })} text={zh} isMac clock={() => STEADY} language={{ value: "system", onChange }} />);

    fireEvent.change(await screen.findByRole("combobox", { name: "语言" }), { target: { value: "en" } });

    expect(onChange).toHaveBeenCalledWith("en");
  });

  it("asks for the password instead of calling an empty one wrong", async () => {
    const api = demoApi({ clock: () => STEADY, status: "needsPassword" });
    const unlock = vi.spyOn(api, "unlock");
    show(api);

    fireEvent.click(await screen.findByRole("button", { name: "打开" }));

    expect((await screen.findByRole("alert")).textContent).toBe("请输入主口令");
    expect(unlock).not.toHaveBeenCalled();
  });
});

describe("a panel row between two ticks", () => {
  const entry = { id: "d1", title: "GitHub", issuer: "GitHub", account: "demo-user", note: "", tags: [], kind: "totp", pinned: true, code: "123456", periodS: 30, validUntilMs: STEADY + 10_000 } as const;
  const onCopy = () => undefined;
  const onHover = () => undefined;
  const props = (changes: Partial<Parameters<typeof samePanelRow>[0]> = {}): Parameters<typeof samePanelRow>[0] => ({
    entry: { ...entry, tags: [] },
    index: 0,
    selected: false,
    copied: false,
    coreNow: STEADY,
    epoch: 1,
    text: zh,
    isMac: true,
    onCopy,
    onHover,
    ...changes,
  });

  it("is drawn again only when its countdown shows another second", () => {
    const base = props();
    expect(samePanelRow(base, { ...base, coreNow: STEADY + 400 })).toBe(true);
    expect(samePanelRow(base, { ...base, coreNow: STEADY + 1_000 })).toBe(false);
  });

  it("is drawn again when it is chosen, copied or its codes are loaded again", () => {
    const base = props();
    expect(samePanelRow(base, { ...base, selected: true })).toBe(false);
    expect(samePanelRow(base, { ...base, copied: true })).toBe(false);
    expect(samePanelRow(base, { ...base, epoch: 2 })).toBe(false);
  });
});
