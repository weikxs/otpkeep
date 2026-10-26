import { fireEvent, render, screen, waitFor, within } from "@testing-library/preact";
import { describe, expect, it, vi } from "vitest";
import { demoMain } from "./demo";
import { mainStrings } from "./i18n";
import { MainApp } from "./MainApp";

const zh = mainStrings("zh-CN");
const STEADY = 1_700_000_020_000;

async function openAdvanced(title: string, api = demoMain({ clock: () => STEADY })) {
  render(<MainApp api={api} text={zh} isMac clock={() => STEADY} />);
  const rows = await screen.findAllByTestId("entry");
  const row = rows.find((r) => r.textContent?.includes(title)) as HTMLElement;
  fireEvent.click(within(row).getByText("编辑"));
  const form = (await screen.findByRole("textbox", { name: "显示名称" })).closest("form") as HTMLElement;
  fireEvent.click(within(form).getByText("服务商、账号、验证码设置与快捷键"));
  return { api, editor: within(form) };
}

describe("the editor's advanced part", () => {
  it("renames the service and account with the rest of the form", async () => {
    const { api, editor } = await openAdvanced("GitHub");
    const update = vi.spyOn(api, "update");

    fireEvent.input(editor.getByLabelText("服务商"), { target: { value: "GitHub Enterprise" } });
    fireEvent.click(editor.getByText("保存"));

    await waitFor(() => expect(update).toHaveBeenCalledWith("a1", { issuer: "GitHub Enterprise" }));
  });

  it("sets the entry's own shortcut and names a default it collides with", async () => {
    const { editor } = await openAdvanced("GitHub");
    expect(editor.getByText(/默认 ⌃⌥⌘/u)).toBeTruthy();

    fireEvent.click(editor.getByText("设置"));
    fireEvent.keyDown(screen.getByTestId("recorder"), { code: "Digit1", ctrlKey: true, altKey: true, metaKey: true });
    expect(await editor.findByText("这个组合是列表第 1 个账号的默认快捷键，请换一个")).toBeTruthy();

    fireEvent.click(editor.getByText("设置"));
    fireEvent.keyDown(screen.getByTestId("recorder"), { code: "KeyG", ctrlKey: true, altKey: true, metaKey: true });
    expect(await screen.findByText("快捷键已设置")).toBeTruthy();
  });

  it("shows the code settings and previews a change before saving it", async () => {
    const { api, editor } = await openAdvanced("GitHub");
    const replace = vi.spyOn(api, "replaceKey");
    expect(await editor.findByText(/按时间（TOTP） · SHA1 · 6 位 · 30 秒/u)).toBeTruthy();

    fireEvent.click(editor.getByText("修改"));
    fireEvent.change(editor.getByLabelText("位数"), { target: { value: "8" } });
    fireEvent.click(editor.getByRole("button", { name: "预览" }));
    expect(await editor.findByText(/按新设置，现在的验证码是/u)).toBeTruthy();
    expect(editor.queryByText(/旧密钥不再生成验证码/u)).toBeNull();

    fireEvent.click(editor.getAllByRole("button", { name: "保存" })[0] as HTMLElement);
    await waitFor(() => expect(replace).toHaveBeenCalledWith("a1", expect.objectContaining({ digits: 8, secret: "" })));
  });

  it("warns that a new secret replaces the old one and points at a bad character", async () => {
    const { editor } = await openAdvanced("GitHub");
    fireEvent.click(await editor.findByText("修改"));

    fireEvent.input(editor.getByLabelText("新密钥"), { target: { value: "JBSW8" } });
    expect(editor.getByText("保存后旧密钥不再生成验证码；请确认网站上也已经更换")).toBeTruthy();
    fireEvent.click(editor.getByRole("button", { name: "预览" }));

    expect(await editor.findByText(/第 5 个字符“8”不合法.*字母 O、I、B、G/u)).toBeTruthy();
  });
});
