import { fireEvent, render, screen, waitFor, within } from "@testing-library/preact";
import { describe, expect, it, vi } from "vitest";
import { demoMain } from "./demo";
import { mainStrings } from "./i18n";
import { MainApp } from "./MainApp";
import type { MainApi } from "./api";

const zh = mainStrings("zh-CN");
// 10 seconds into a 30-second step.
const STEADY = 1_700_000_020_000;
const DEMO = "JBSWY3DPEHPK3PXP";

function show(api: MainApi) {
  return render(<MainApp api={api} text={zh} isMac clock={() => STEADY} />);
}

async function dialog() {
  return within(await screen.findByRole("dialog"));
}

describe("MainApp", () => {
  it("lists every account with its note and code", async () => {
    show(demoMain({ clock: () => STEADY }));

    const rows = await screen.findAllByTestId("entry");
    expect(rows).toHaveLength(4);
    expect(screen.getByText("4 个账号")).toBeTruthy();
    expect(screen.getByText("备用码在抽屉里")).toBeTruthy();
    const codes = screen.getAllByTestId("code").map((c) => c.textContent);
    expect(codes[0]).toMatch(/^\d{3} \d{3}$/u);
    expect(codes).toContainEqual(expect.stringMatching(/^[2-9B-Y]{5}$/u));
  });

  it("copies on click and generates a counter code only when asked", async () => {
    const api = demoMain({ clock: () => STEADY });
    show(api);
    const rows = await screen.findAllByTestId("entry");

    fireEvent.click(within(rows[0] as HTMLElement).getByRole("button", { name: /GitHub/u }));
    await waitFor(() => expect(api.copied).toEqual(["a1"]));
    expect(screen.getByRole("status").textContent).toBe("已复制 · 30 秒后从剪贴板清除");

    fireEvent.click(screen.getByText("生成"));
    await waitFor(() => expect(api.copied).toEqual(["a1", "a4"]));
  });

  it("edits the name and note in place", async () => {
    show(demoMain({ clock: () => STEADY }));
    const rows = await screen.findAllByTestId("entry");

    fireEvent.click(within(rows[0] as HTMLElement).getByText("编辑"));
    fireEvent.input(screen.getByLabelText("显示名称"), { target: { value: "GitHub 工作" } });
    fireEvent.input(screen.getByLabelText("备注"), { target: { value: "公司账号" } });
    fireEvent.click(screen.getByText("保存"));

    expect(await screen.findByText("GitHub 工作")).toBeTruthy();
    expect(screen.getByText("公司账号")).toBeTruthy();
  });

  it("explains a refused name", async () => {
    show(demoMain({ clock: () => STEADY }));
    const rows = await screen.findAllByTestId("entry");

    fireEvent.click(within(rows[0] as HTMLElement).getByText("编辑"));
    fireEvent.input(screen.getByLabelText("显示名称"), { target: { value: " " } });
    fireEvent.click(screen.getByText("保存"));

    expect((await screen.findByRole("alert")).textContent).toBe("名称不能为空");
  });

  it("searches", async () => {
    show(demoMain({ clock: () => STEADY }));
    await screen.findAllByTestId("entry");

    fireEvent.input(screen.getByRole("searchbox"), { target: { value: "steam" } });

    await waitFor(() => expect(screen.getAllByTestId("entry")).toHaveLength(1));
  });

  it("welcomes a first start with three ways to add", async () => {
    show(demoMain({ clock: () => STEADY, empty: true }));

    expect(await screen.findByText("添加第一个账号")).toBeTruthy();
    fireEvent.click(screen.getByText("手动输入"));

    expect(await screen.findByRole("dialog")).toBeTruthy();
    expect(screen.getByLabelText("密钥")).toBeTruthy();
  });

  it("adds a pasted link after the preview and says what it skipped", async () => {
    show(demoMain({ clock: () => STEADY, empty: true }));
    fireEvent.click(await screen.findByText("粘贴链接"));
    const add = await dialog();

    fireEvent.input(add.getByLabelText("粘贴链接"), {
      target: { value: `otpauth://totp/Demo:alex?secret=${DEMO}\nphonefactor://activate_account?code=1` },
    });
    fireEvent.click(add.getByText("读取"));

    expect(await add.findByText("将添加 1 个账号")).toBeTruthy();
    expect(add.getByText(/第 2 行：这是微软验证器的推送码/u)).toBeTruthy();
    expect(add.getAllByTestId("preview-row")[0]?.textContent).toMatch(/\d{3} \d{3}/u);
    fireEvent.click(add.getByText("添加 1 个"));

    expect(await screen.findByText("已添加 1 个账号")).toBeTruthy();
    expect(screen.getAllByTestId("entry")).toHaveLength(1);
  });

  it("leaves an account already present unchosen", async () => {
    show(demoMain({ clock: () => STEADY }));
    fireEvent.click(await screen.findByText("+ 添加"));
    const add = await dialog();

    fireEvent.input(add.getByLabelText("粘贴链接"), { target: { value: "otpauth://totp/Again:x?secret=GH" } });
    fireEvent.click(add.getByText("读取"));

    expect(await add.findByText("没有可添加的账号")).toBeTruthy();
    expect(add.getByText(/已存在：GitHub/u)).toBeTruthy();
    const box = add.getByRole("checkbox") as HTMLInputElement;
    expect(box.checked).toBe(false);
    fireEvent.click(box);
    expect(add.getByText("添加 1 个")).toBeTruthy();
  });

  it("asks for a backup's password", async () => {
    const api = demoMain({ clock: () => STEADY, empty: true });
    show(api);
    fireEvent.click(await screen.findByText("图片或文件"));
    const add = await dialog();
    const backup = new File([new TextEncoder().encode("OKV\0rest")], "demo.otpkeep");

    fireEvent.change(add.getByLabelText("选择图片或文件"), { target: { files: [backup] } });

    expect(await add.findByText("这是 OTPKeep 备份")).toBeTruthy();
    fireEvent.input(add.getByLabelText("备份口令"), { target: { value: "wrong" } });
    fireEvent.click(add.getByText("打开"));
    expect(await add.findByText("备份口令不正确，或备份文件已被改动")).toBeTruthy();
    fireEvent.input(add.getByLabelText("备份口令"), { target: { value: "demo" } });
    fireEvent.click(add.getByText("打开"));
    expect(await add.findByText("Backed up")).toBeTruthy();
  });

  it("names the bad character of a typed secret and its likely mistake", async () => {
    show(demoMain({ clock: () => STEADY, empty: true }));
    fireEvent.click(await screen.findByText("手动输入"));
    const add = await dialog();

    fireEvent.input(add.getByLabelText("服务商"), { target: { value: "Example" } });
    fireEvent.input(add.getByLabelText("密钥"), { target: { value: "JBSW0Y3DP" } });
    fireEvent.click(add.getByText("预览"));

    expect((await add.findByRole("alert")).textContent).toMatch(/第 5 个字符“0”不合法.*字母 O/u);
  });

  it("adds a typed secret after showing its code", async () => {
    show(demoMain({ clock: () => STEADY, empty: true }));
    fireEvent.click(await screen.findByText("手动输入"));
    const add = await dialog();

    fireEvent.input(add.getByLabelText("服务商"), { target: { value: "Example" } });
    fireEvent.input(add.getByLabelText("密钥"), { target: { value: "jbsw y3dp ehpk 3pxp" } });
    fireEvent.click(add.getByText("预览"));

    expect(await add.findByText("添加前可以先到网站上核对下面的验证码")).toBeTruthy();
    fireEvent.click(add.getByText("添加 1 个"));
    expect(await screen.findByText("Example")).toBeTruthy();
  });

  it("cancels a preview when the dialog closes", async () => {
    const api = demoMain({ clock: () => STEADY, empty: true });
    const cancel = vi.spyOn(api, "cancel");
    show(api);
    fireEvent.click(await screen.findByText("粘贴链接"));
    const add = await dialog();
    fireEvent.input(add.getByLabelText("粘贴链接"), { target: { value: `otpauth://totp/A:b?secret=${DEMO}` } });
    fireEvent.click(add.getByText("读取"));
    await add.findByText("将添加 1 个账号");

    fireEvent.click(add.getByLabelText("关闭"));

    expect(cancel).toHaveBeenCalledOnce();
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("reads a file dropped on the window", async () => {
    show(demoMain({ clock: () => STEADY }));
    const rows = await screen.findAllByTestId("entry");
    const picture = new File([new Uint8Array([0x89, 0x50, 0x4e, 0x47])], "qr.png", { type: "image/png" });
    const main = (rows[0] as HTMLElement).closest(".main") as HTMLElement;

    fireEvent.drop(main, { dataTransfer: { files: [picture] } });

    expect(await screen.findByText("From picture")).toBeTruthy();
  });

  it("asks for the master password first", async () => {
    show(demoMain({ clock: () => STEADY, needsPassword: true }));

    fireEvent.input(await screen.findByLabelText("主口令"), { target: { value: "demo" } });
    fireEvent.click(screen.getByText("打开"));

    expect(await screen.findAllByTestId("entry")).toHaveLength(4);
  });
});

describe("mainStrings", () => {
  it("has English for every message", () => {
    const en = mainStrings("en-US");
    expect(en.errors.UNKNOWN_FORMAT).toMatch(/cannot read/u);
    expect(en.reasons.MICROSOFT_PUSH).toMatch(/Microsoft/u);
    expect(en.invalid.DIGITS).toBe("Digits must be 6–8");
  });
});
