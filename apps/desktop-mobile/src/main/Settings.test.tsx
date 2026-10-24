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

  it("shows the version and that it works offline", async () => {
    show();
    const dialog = await openSettings();

    expect(await dialog.findByText("OTPKeep 0.0.1 · 完全在本机工作，不联网")).toBeTruthy();
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
