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
    expect(screen.getByText(/备用码在抽屉里/u)).toBeTruthy();
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
    expect(screen.getByText(/公司账号/u)).toBeTruthy();
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

describe("copying from the management page", () => {
  it("copies a code and says when the clipboard clears", async () => {
    const api = demoGate();
    const copy = vi.spyOn(api, "copy");
    show(api);

    fireEvent.click((await card("GitHub")).getByText("复制"));

    expect(await screen.findByText("已复制 · 30 秒后从剪贴板清除")).toBeTruthy();
    expect(copy).toHaveBeenCalledWith("d1");
    expect(await navigator.clipboard.readText()).toMatch(/^\d{6}$/u);
  });

  it("explains that an HOTP code is generated in OTPKeep first", async () => {
    show(demoGate());

    fireEvent.click((await card("Bank token")).getByText("复制"));

    expect(await screen.findByText("计数器型（HOTP）账号需要先在 OTPKeep 面板中生成验证码")).toBeTruthy();
  });
});

describe("codes on the management page", () => {
  it("shows each account's code with its countdown, and the note on the second line", async () => {
    show(demoGate());

    const github = await card("GitHub");
    expect(github.getByTestId("code").textContent).toMatch(/^\d{3} \d{3}$/u);
    expect(github.getByLabelText(/^\d+ 秒$/u)).toBeTruthy();
    expect((await card("邮箱")).getByTitle("示例邮箱 · demo@example.com · 备用码在抽屉里")).toBeTruthy();
    expect((await card("Bank token")).getByText("在 OTPKeep 中生成")).toBeTruthy();
  });

  it("fetches new codes once the shown ones end", async () => {
    // One second before the 30-second step ends (steps start at multiples of 30 000 ms).
    let now = 1_700_000_039_000;
    const api = demoGate(() => now);
    const entries = vi.spyOn(api, "entries");
    show(api);
    await card("GitHub");
    const before = entries.mock.calls.length;

    now += 2000;

    await waitFor(() => expect(entries.mock.calls.length).toBeGreaterThan(before), { timeout: 2000 });
  });
});

describe("language on the management page", () => {
  it("shows the language menu in the header and reports a choice", async () => {
    const onChange = vi.fn();
    render(<GateApp connect={() => Promise.resolve(demoGate())} text={zh} isMac language={{ value: "system", onChange }} />);

    fireEvent.change(await screen.findByRole("combobox", { name: "语言" }), { target: { value: "en" } });

    expect(onChange).toHaveBeenCalledWith("en");
  });
});
