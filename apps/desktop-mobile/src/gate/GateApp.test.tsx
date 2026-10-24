import { fireEvent, render, screen, waitFor, within } from "@testing-library/preact";
import { describe, expect, it, vi } from "vitest";
import { GateFailure, type GateApi } from "./api";
import { demoGate } from "./demo";
import { GateApp } from "./GateApp";
import { gateStrings } from "./i18n";

const zh = gateStrings("zh-CN");

function show(api: GateApi | Promise<GateApi>) {
  return render(<GateApp connect={() => Promise.resolve(api)} text={zh} isMac />);
}

async function card(title: string) {
  const cards = await screen.findAllByTestId("card");
  const found = cards.find((c) => c.textContent?.includes(title));
  if (!found) throw new Error(`no card ${title}`);
  return within(found);
}

describe("GateApp", () => {
  it("lists the accounts with notes and shortcuts", async () => {
    show(demoGate());

    expect(await screen.findAllByTestId("card")).toHaveLength(3);
    expect(screen.getByText("3 个账号")).toBeTruthy();
    expect(screen.getByText("备用码在抽屉里")).toBeTruthy();
    expect(screen.getByText("⌃⌥⌘M")).toBeTruthy();
    expect(screen.getByText("默认 ⌃⌥⌘1（按列表位置）")).toBeTruthy();
  });

  it("searches", async () => {
    show(demoGate());
    await screen.findAllByTestId("card");

    fireEvent.input(screen.getByRole("searchbox"), { target: { value: "bank" } });

    await waitFor(() => expect(screen.getAllByTestId("card")).toHaveLength(1));
  });

  it("edits the name and the note", async () => {
    show(demoGate());
    const github = await card("GitHub");

    fireEvent.click(github.getByText("编辑"));
    fireEvent.input(github.getByLabelText("显示名称"), { target: { value: "GitHub 工作" } });
    fireEvent.input(github.getByLabelText("备注"), { target: { value: "公司账号" } });
    fireEvent.click(github.getByText("保存"));

    expect(await screen.findByText("GitHub 工作")).toBeTruthy();
    expect(screen.getByText("公司账号")).toBeTruthy();
    expect(screen.getByRole("status").textContent).toBe("已保存");
  });

  it("explains a refused name and keeps the form open", async () => {
    show(demoGate());
    const github = await card("GitHub");

    fireEvent.click(github.getByText("编辑"));
    fireEvent.input(github.getByLabelText("显示名称"), { target: { value: " " } });
    fireEvent.click(github.getByText("保存"));

    expect((await screen.findByRole("alert")).textContent).toBe("名称不能为空");
    expect(github.getByLabelText("显示名称")).toBeTruthy();
  });

  it("cancels an edit without saving", async () => {
    const api = demoGate();
    const edit = vi.spyOn(api, "edit");
    show(api);
    const github = await card("GitHub");

    fireEvent.click(github.getByText("编辑"));
    fireEvent.click(github.getByText("取消"));

    expect(edit).not.toHaveBeenCalled();
  });

  it("records a shortcut from the key press", async () => {
    show(demoGate());
    const bank = await card("Bank token");

    fireEvent.click(bank.getByText("设置"));
    fireEvent.keyDown(screen.getByTestId("recorder"), { code: "KeyB", ctrlKey: true, altKey: true, metaKey: true });

    expect(await screen.findByText("⌃⌥⌘B")).toBeTruthy();
    expect(screen.getByRole("status").textContent).toBe("快捷键已设置，按下即可复制验证码");
  });

  it("waits for a real modifier while recording", async () => {
    const api = demoGate();
    const set = vi.spyOn(api, "setShortcut");
    show(api);
    const bank = await card("Bank token");

    fireEvent.click(bank.getByText("设置"));
    fireEvent.keyDown(screen.getByTestId("recorder"), { code: "AltLeft", altKey: true });
    fireEvent.keyDown(screen.getByTestId("recorder"), { code: "KeyB", shiftKey: true });

    expect(set).not.toHaveBeenCalled();
    expect(screen.getByText(/至少一个/u)).toBeTruthy();
  });

  it("cancels recording with Esc", async () => {
    const api = demoGate();
    const set = vi.spyOn(api, "setShortcut");
    show(api);
    const bank = await card("Bank token");

    fireEvent.click(bank.getByText("设置"));
    fireEvent.keyDown(screen.getByTestId("recorder"), { code: "Escape" });

    expect(set).not.toHaveBeenCalled();
    expect(screen.queryByTestId("recorder")).toBeNull();
  });

  it("names who already uses a shortcut", async () => {
    show(demoGate());
    const bank = await card("Bank token");

    fireEvent.click(bank.getByText("设置"));
    fireEvent.keyDown(screen.getByTestId("recorder"), { code: "KeyM", ctrlKey: true, altKey: true, metaKey: true });

    expect((await screen.findByRole("alert")).textContent).toBe("这个组合已被“邮箱”使用");
  });

  it("refuses a row's default shortcut", async () => {
    show(demoGate());
    const bank = await card("Bank token");

    fireEvent.click(bank.getByText("设置"));
    fireEvent.keyDown(screen.getByTestId("recorder"), { code: "Digit1", ctrlKey: true, altKey: true, metaKey: true });

    expect((await screen.findByRole("alert")).textContent).toBe("这个组合是列表第 1 个账号的默认快捷键，请换一个");
  });

  it("clears a shortcut", async () => {
    show(demoGate());
    const mail = await card("邮箱");

    fireEvent.click(mail.getByText("清除"));

    await waitFor(() => expect(screen.queryByText("⌃⌥⌘M")).toBeNull());
    expect(screen.getByRole("status").textContent).toBe("已清除快捷键");
  });

  it("shows when the link was used already", async () => {
    render(<GateApp connect={() => Promise.reject(new GateFailure("LINK_USED"))} text={zh} isMac />);

    expect(await screen.findByText("此链接已被使用")).toBeTruthy();
  });

  it("shows when the session ended during use", async () => {
    const api = demoGate();
    show({ ...api, setShortcut: () => Promise.reject(new GateFailure("NO_SESSION")) });
    const bank = await card("Bank token");

    fireEvent.click(bank.getByText("设置"));
    fireEvent.keyDown(screen.getByTestId("recorder"), { code: "KeyB", ctrlKey: true, altKey: true, metaKey: true });

    expect(await screen.findByText("会话已结束")).toBeTruthy();
  });

  it("asks for the master password first", async () => {
    show({ ...demoGate(), entries: () => Promise.reject(new GateFailure("NEEDS_PASSWORD")) });

    expect(await screen.findByText("请先在 OTPKeep 中输入主口令，然后刷新本页。")).toBeTruthy();
  });

  it("falls back to English", () => {
    expect(gateStrings("en-GB").count(1)).toBe("1 account");
  });
});
