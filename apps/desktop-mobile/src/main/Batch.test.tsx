import { fireEvent, render, screen, waitFor, within } from "@testing-library/preact";
import { describe, expect, it } from "vitest";
import { demoMain } from "./demo";
import { mainStrings } from "./i18n";
import { MainApp } from "./MainApp";

const zh = mainStrings("zh-CN");
const STEADY = 1_700_000_020_000;

async function selectRows(...titles: string[]) {
  const api = demoMain({ clock: () => STEADY });
  render(<MainApp api={api} text={zh} isMac clock={() => STEADY} />);
  fireEvent.click(await screen.findByText("选择"));
  for (const title of titles) fireEvent.click(screen.getByRole("checkbox", { name: title }));
  return { api, bar: within(screen.getByRole("toolbar")) };
}

describe("batch actions", () => {
  it("chooses rows instead of copying them", async () => {
    const { api, bar } = await selectRows();
    const row = screen.getAllByTestId("entry")[0] as HTMLElement;

    fireEvent.click(within(row).getByRole("button", { name: /GitHub/u }));

    expect(bar.getByText("已选 1 个")).toBeTruthy();
    expect(api.copied).toEqual([]);
  });

  it("adds a tag to the chosen accounts", async () => {
    await selectRows("GitHub", "Steam");
    const bar = within(screen.getByRole("toolbar"));

    fireEvent.click(bar.getByText("加标签"));
    fireEvent.input(bar.getByLabelText("要添加的标签"), { target: { value: "游戏" } });
    fireEvent.click(bar.getByText("应用"));

    expect(await screen.findByText("已修改 2 个账号")).toBeTruthy();
    expect(screen.getAllByText("游戏", { selector: ".tag" })).toHaveLength(2);
  });

  it("removes a tag the chosen accounts have", async () => {
    const { bar } = await selectRows("邮箱");

    fireEvent.click(bar.getByText("去标签"));
    fireEvent.change(bar.getByLabelText("要去掉的标签"), { target: { value: "工作" } });
    fireEvent.click(bar.getByText("应用"));

    expect(await screen.findByText("已修改 1 个账号")).toBeTruthy();
    await waitFor(() => expect(screen.queryByText("工作", { selector: ".tag" })).toBeNull());
  });

  it("moves the chosen accounts to the trash after confirming", async () => {
    const { bar } = await selectRows("GitHub", "邮箱");

    fireEvent.click(bar.getByText("移入回收站"));
    expect(bar.getByText("把 2 个账号移入回收站？30 天内可以恢复")).toBeTruthy();
    fireEvent.click(bar.getByRole("button", { name: "移入回收站" }));

    expect(await screen.findByText("已把 2 个账号移入回收站")).toBeTruthy();
    expect(screen.getAllByTestId("entry")).toHaveLength(2);
    expect(screen.getByText("回收站（2）")).toBeTruthy();
  });

  it("selects all and leaves selection mode", async () => {
    const { bar } = await selectRows();

    fireEvent.click(bar.getByText("全选"));
    expect(bar.getByText("已选 4 个")).toBeTruthy();
    fireEvent.click(bar.getByText("完成"));

    expect(screen.queryByRole("toolbar")).toBeNull();
    expect(screen.queryAllByRole("checkbox")).toHaveLength(0);
  });

  it("explains an empty tag", async () => {
    const { bar } = await selectRows("GitHub");

    fireEvent.click(bar.getByText("加标签"));
    fireEvent.click(bar.getByText("应用"));

    expect((await bar.findByRole("alert")).textContent).toBe("请填写标签");
  });
});
