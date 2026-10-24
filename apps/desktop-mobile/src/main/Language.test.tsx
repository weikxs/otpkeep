import { fireEvent, render, screen } from "@testing-library/preact";
import { describe, expect, it, vi } from "vitest";
import { demoMain } from "./demo";
import { Root } from "./Root";

const STEADY = 1_700_000_020_000;

function show(options: Parameters<typeof demoMain>[0] = {}) {
  const api = demoMain({ clock: () => STEADY, ...options });
  render(<Root api={api} systemLanguage="zh-CN" isMac clock={() => STEADY} />);
  return api;
}

describe("language menu", () => {
  it("follows the system until a language is chosen, then saves it for every window", async () => {
    const api = show();
    const menu = await screen.findByRole("combobox", { name: "语言" });
    expect((menu as HTMLSelectElement).value).toBe("system");

    fireEvent.change(menu, { target: { value: "en" } });

    expect(await screen.findByRole("button", { name: "Settings" })).toBeTruthy();
    expect((await api.settings()).language).toBe("en");
    fireEvent.change(screen.getByRole("combobox", { name: "Language" }), { target: { value: "system" } });
    expect(await screen.findByRole("button", { name: "设置" })).toBeTruthy();
  });

  it("names each language in itself", async () => {
    show();
    const menu = await screen.findByRole("combobox", { name: "语言" });

    const names = [...(menu as HTMLSelectElement).options].map((o) => o.textContent);

    expect(names).toEqual(["跟随系统", "简体中文", "English"]);
  });

  it("is on the master password screen too", async () => {
    show({ needsPassword: true });

    fireEvent.change(await screen.findByRole("combobox", { name: "语言" }), { target: { value: "en" } });

    expect(await screen.findByText("Enter the master password to open your data")).toBeTruthy();
  });
});

describe("master password screen", () => {
  it("asks for the password instead of calling an empty one wrong", async () => {
    const api = show({ needsPassword: true });
    const unlock = vi.spyOn(api, "unlock");
    await screen.findByLabelText("主口令");

    fireEvent.click(screen.getByRole("button", { name: "打开" }));

    expect((await screen.findByRole("alert")).textContent).toBe("请输入主口令");
    expect(unlock).not.toHaveBeenCalled();
  });

  it("still says so when the password is wrong", async () => {
    show({ needsPassword: true });

    fireEvent.input(await screen.findByLabelText("主口令"), { target: { value: "nope" } });
    fireEvent.click(screen.getByRole("button", { name: "打开" }));

    expect((await screen.findByRole("alert")).textContent).toBe("主口令不正确");
  });
});
