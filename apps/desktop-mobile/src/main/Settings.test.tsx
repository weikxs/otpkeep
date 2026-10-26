import { fireEvent, render, screen, waitFor, within } from "@testing-library/preact";
import { afterEach, describe, expect, it, vi } from "vitest";
import { demoMain } from "./demo";
import { Root } from "./Root";

const STEADY = 1_700_000_020_000;

function show() {
  const api = demoMain({ clock: () => STEADY });
  render(<Root api={api} systemLanguage="zh-CN" isMac clock={() => STEADY} />);
  return api;
}

async function openSettings() {
  fireEvent.click(await screen.findByRole("button", { name: "设置" }));
  return within(await screen.findByRole("dialog"));
}

afterEach(() => {
  delete document.documentElement.dataset.theme;
});

describe("settings", () => {
  it("switches the theme at once", async () => {
    show();
    const dialog = await openSettings();

    fireEvent.change(await dialog.findByLabelText("主题"), { target: { value: "dark" } });

    await waitFor(() => expect(document.documentElement.dataset.theme).toBe("dark"));
  });

  it("switches the language at once", async () => {
    show();
    const dialog = await openSettings();

    fireEvent.change(await dialog.findByLabelText("语言"), { target: { value: "en" } });

    expect(await screen.findByRole("button", { name: "Settings" })).toBeTruthy();
    expect(screen.getByText("4 accounts")).toBeTruthy();
  });

  it("turns clipboard clearing off, and copying says so", async () => {
    const api = show();
    const dialog = await openSettings();

    fireEvent.change(await dialog.findByLabelText(/复制后清除剪贴板/u), { target: { value: "0" } });
    await waitFor(async () => expect((await api.settings()).clipboardClearS).toBe(0));
    fireEvent.click(dialog.getByLabelText("关闭"));
    fireEvent.click(screen.getAllByRole("button", { name: /GitHub/u })[0] as HTMLElement);

    expect(await screen.findByText("已复制")).toBeTruthy();
  });

  it("turns the row shortcuts off", async () => {
    const api = show();
    const set = vi.spyOn(api, "setSettings");
    const dialog = await openSettings();

    fireEvent.click(await dialog.findByLabelText(/前 10 个账号的快捷键/u));

    await waitFor(() => expect(set).toHaveBeenCalledWith({ rowShortcuts: false }));
  });

  it("lets scripts get codes from the running app", async () => {
    const api = show();
    const dialog = await openSettings();
    const allow = (await dialog.findByLabelText(/允许脚本/u)) as HTMLInputElement;

    expect(allow.checked).toBe(true);
    fireEvent.click(allow);

    await waitFor(async () => expect((await api.settings()).scripts).toBe(false));
  });

  it("sets how many accounts the menu bar menu shows", async () => {
    const api = show();
    const dialog = await openSettings();
    const count = await dialog.findByLabelText(/菜单栏菜单中的账号数/u);

    expect((count as HTMLSelectElement).value).toBe("30");
    fireEvent.change(count, { target: { value: "15" } });

    await waitFor(async () => expect((await api.settings()).menuAccounts).toBe(15));
  });

  it("shows the version and that it works offline", async () => {
    show();
    const dialog = await openSettings();

    expect(await dialog.findByText("OTPKeep 0.1.0 · 完全在本机工作，不联网")).toBeTruthy();
  });

  it("explains a refused value", async () => {
    const api = show();
    vi.spyOn(api, "setSettings").mockRejectedValueOnce(
      new (await import("./api")).CommandFailure("INVALID", "CLIPBOARD"),
    );
    const dialog = await openSettings();

    fireEvent.change(await dialog.findByLabelText(/复制后清除剪贴板/u), { target: { value: "10" } });

    expect((await dialog.findByRole("alert")).textContent).toBe("清除时间应为 10–120 秒，或选择不清除");
  });
});

describe("search panel shortcut", () => {
  it("is changed, refused when taken, turned off and reset", async () => {
    const api = show();
    const dialog = await openSettings();
    expect(await dialog.findByText("⌃⌥⌘Space")).toBeTruthy();

    fireEvent.click(dialog.getByText("更改"));
    fireEvent.keyDown(screen.getByTestId("recorder"), { code: "Digit1", ctrlKey: true, altKey: true, metaKey: true });
    expect(await dialog.findByText("这个组合是列表第 1 个账号的默认快捷键，请换一个")).toBeTruthy();

    fireEvent.click(dialog.getByText("更改"));
    fireEvent.keyDown(screen.getByTestId("recorder"), { code: "KeyK", ctrlKey: true, altKey: true, metaKey: true });
    await waitFor(async () => expect((await api.settings()).searchShortcut).toBe("Control+Alt+Super+KeyK"));

    fireEvent.click(dialog.getByText("清除"));
    expect(await dialog.findByText("已关闭")).toBeTruthy();

    fireEvent.click(dialog.getByText("恢复默认 ⌃⌥⌘Space"));
    await waitFor(async () => expect((await api.settings()).searchShortcut).toBe("control+alt+super+Space"));
  });
});

describe("typing codes", () => {
  it("shows that permission is needed and asks for it", async () => {
    const api = show();
    const ask = vi.spyOn(api, "askTypingPermission");
    const dialog = await openSettings();

    expect(await dialog.findByText('需要"辅助功能"授权')).toBeTruthy();
    fireEvent.click(dialog.getByText("去授权"));

    expect(await dialog.findByText("可以使用")).toBeTruthy();
    expect(ask).toHaveBeenCalled();
    expect(dialog.queryByText("去授权")).toBeNull();
  });
});
