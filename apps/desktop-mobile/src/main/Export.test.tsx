import { fireEvent, render, screen, waitFor, within } from "@testing-library/preact";
import { describe, expect, it, vi } from "vitest";
import { CommandFailure } from "./api";
import { demoMain } from "./demo";
import { mainStrings } from "./i18n";
import { MainApp } from "./MainApp";
import { QR_SHOWN_S, SingleQr } from "./QrDialogs";

const zh = mainStrings("zh-CN");
const STEADY = 1_700_000_020_000;

async function openExport(api = demoMain({ clock: () => STEADY })) {
  render(<MainApp api={api} text={zh} isMac clock={() => STEADY} />);
  fireEvent.click(await screen.findByRole("button", { name: "导出" }));
  return { api, dialog: within(await screen.findByRole("dialog", { name: "导出" })) };
}

describe("export", () => {
  it("asks for the backup password twice", async () => {
    const { api, dialog } = await openExport();
    const save = vi.spyOn(api, "exportFile");
    expect(dialog.getByText("全部 4 个账号")).toBeTruthy();

    fireEvent.input(dialog.getByLabelText("备份口令"), { target: { value: "口令一" } });
    fireEvent.input(dialog.getByLabelText("再输一次"), { target: { value: "口令二" } });
    fireEvent.click(dialog.getByText("导出…"));
    expect((await dialog.findByRole("alert")).textContent).toBe("两次输入的口令不一致");
    expect(save).not.toHaveBeenCalled();

    fireEvent.input(dialog.getByLabelText("备份口令"), { target: { value: "口令一" } });
    fireEvent.input(dialog.getByLabelText("再输一次"), { target: { value: "口令一" } });
    fireEvent.click(dialog.getByText("导出…"));

    expect(await screen.findByText("已导出 4 个账号到“otpkeep-backup.otpkeep”")).toBeTruthy();
    expect(save).toHaveBeenCalledWith("backup", null, "口令一");
  });

  it("warns that plaintext formats are confirmed by the system and lists what was left out", async () => {
    const { dialog } = await openExport();

    fireEvent.click(dialog.getByText("GoldenPassport 数据（.secrets）"));
    expect(dialog.getByText("这个格式不加密，导出前系统会再请你确认一次")).toBeTruthy();
    fireEvent.click(dialog.getByText("导出…"));

    expect(await dialog.findByText("2 个账号未导出")).toBeTruthy();
    expect(dialog.getByText(/Steam：Steam 令牌，该格式不支持/u)).toBeTruthy();
    expect(dialog.getByText(/Bank token：计数器型，该格式不支持/u)).toBeTruthy();
  });

  it("is quiet when the person says no in the native dialog", async () => {
    const api = demoMain({ clock: () => STEADY });
    vi.spyOn(api, "exportFile").mockRejectedValueOnce(new CommandFailure("CANCELLED"));
    const { dialog } = await openExport(api);

    fireEvent.click(dialog.getByText("otpauth 链接清单（.txt）"));
    fireEvent.click(dialog.getByText("导出…"));

    await waitFor(() => expect(api.exportFile).toHaveBeenCalled());
    expect(dialog.queryByRole("alert")).toBeNull();
  });

  it("shows Google transfer codes one after another, with Steam on its own", async () => {
    const { dialog } = await openExport();

    fireEvent.click(dialog.getByText("Google 验证器转移码"));
    fireEvent.click(dialog.getByText("显示二维码…"));

    const codes = within(await screen.findByRole("dialog", { name: "Google 验证器转移码" }));
    expect(codes.getByText("第 1 / 2 张")).toBeTruthy();
    expect((codes.getByAltText("第 1 / 2 张") as HTMLImageElement).src).toMatch(/^data:image\/svg\+xml/u);
    fireEvent.click(codes.getByText("下一张"));
    expect(codes.getByText("第 2 / 2 张")).toBeTruthy();
    expect(codes.getByText(/Steam · Steam 令牌/u)).toBeTruthy();
  });

  it("exports only the chosen accounts from the batch bar", async () => {
    const api = demoMain({ clock: () => STEADY });
    const save = vi.spyOn(api, "exportFile");
    render(<MainApp api={api} text={zh} isMac clock={() => STEADY} />);
    fireEvent.click(await screen.findByText("选择"));
    fireEvent.click(screen.getByRole("checkbox", { name: "GitHub" }));

    fireEvent.click(within(screen.getByRole("toolbar")).getByText("导出所选"));
    const dialog = within(await screen.findByRole("dialog", { name: "导出" }));
    expect(dialog.getByText("所选的 1 个账号")).toBeTruthy();
    fireEvent.click(dialog.getByText("otpauth 链接清单（.txt）"));
    fireEvent.click(dialog.getByText("导出…"));

    await waitFor(() => expect(save).toHaveBeenCalledWith("links", ["a1"], undefined));
  });

  it("closes a single QR code by itself after a minute", async () => {
    const start = STEADY;
    let now = start;
    const onClose = vi.fn();
    render(<SingleQr qr={{ svg: "<svg/>" }} title="GitHub" text={zh} onClose={onClose} clock={() => now} />);
    expect(screen.getByRole("timer").textContent).toBe(`${QR_SHOWN_S} 秒后自动关闭`);

    now = start + QR_SHOWN_S * 1000 + 1;

    await waitFor(() => expect(onClose).toHaveBeenCalled());
  });

  it("shows one account's QR code from the editor", async () => {
    render(<MainApp api={demoMain({ clock: () => STEADY })} text={zh} isMac clock={() => STEADY} />);
    const rows = await screen.findAllByTestId("entry");
    fireEvent.click(within(rows[0] as HTMLElement).getByText("编辑"));
    fireEvent.click(await screen.findByText("服务商、账号、验证码设置与快捷键"));

    fireEvent.click(screen.getByText("显示二维码"));

    expect(await screen.findByRole("dialog", { name: "账号二维码" })).toBeTruthy();
  });
});
