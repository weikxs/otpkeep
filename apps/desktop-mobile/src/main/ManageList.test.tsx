import { fireEvent, render, screen, waitFor, within } from "@testing-library/preact";
import { describe, expect, it } from "vitest";
import type { MainApi } from "./api";
import { demoMain } from "./demo";
import { ENTRY_DRAG_TYPE } from "./EntryRow";
import { mainStrings } from "./i18n";
import { MainApp } from "./MainApp";

const zh = mainStrings("zh-CN");
const STEADY = 1_700_000_020_000;

function show(api: MainApi = demoMain({ clock: () => STEADY })) {
  render(<MainApp api={api} text={zh} isMac clock={() => STEADY} />);
  return api;
}

/** The titles in list order; a row being edited shows its name field instead. */
function titles() {
  return screen.getAllByTestId("entry").map((row) => {
    const field = row.querySelector<HTMLInputElement>("input[maxlength='100']");
    return field ? field.value : (row.querySelector(".entry-title-text")?.textContent ?? "");
  });
}

async function editRow(title: string) {
  const rows = await screen.findAllByTestId("entry");
  const row = rows.find((r) => r.textContent?.includes(title)) as HTMLElement;
  fireEvent.click(within(row).getByText("编辑"));
  return within(await screen.findByRole("textbox", { name: "显示名称" }).then((i) => i.closest("form") as HTMLElement));
}

describe("keeping the list in order", () => {
  it("pins an entry to the top under its own heading", async () => {
    show();
    const editor = await editRow("Steam");

    fireEvent.click(editor.getByLabelText("置顶"));
    fireEvent.click(editor.getByText("保存"));

    await waitFor(() => expect((titles())[0]).toBe("Steam"));
    expect(screen.getByText("置顶", { selector: ".group-title" })).toBeTruthy();
    expect(screen.getByText("其他", { selector: ".group-title" })).toBeTruthy();
  });

  it("moves an entry one place or to the ends", async () => {
    show();
    const editor = await editRow("Bank token");

    fireEvent.click(editor.getByText("上移"));
    await waitFor(() => expect(titles()).toEqual(["GitHub", "邮箱", "Bank token", "Steam"]));

    const top = editor.getByText("移到最前") as HTMLButtonElement;
    await waitFor(() => expect(top.disabled).toBe(false));
    fireEvent.click(top);
    await waitFor(() => expect((titles())[0]).toBe("Bank token"));
  });

  it("reorders by dragging one row onto another", async () => {
    show();
    const rows = await screen.findAllByTestId("entry");
    const data = new Map<string, string>();
    const transfer = {
      types: [ENTRY_DRAG_TYPE],
      setData: (type: string, value: string) => data.set(type, value),
      getData: (type: string) => data.get(type) ?? "",
    };

    // The test page has no layout, so the pointer counts as in the lower half: the row goes after the target.
    fireEvent.dragStart(rows[3] as HTMLElement, { dataTransfer: transfer });
    fireEvent.dragOver(rows[0] as HTMLElement, { dataTransfer: transfer });
    fireEvent.drop(rows[0] as HTMLElement, { dataTransfer: transfer });

    await waitFor(() => expect(titles()).toEqual(["GitHub", "Bank token", "邮箱", "Steam"]));
  });

  it("adds tags and filters the list by one", async () => {
    show();
    const editor = await editRow("GitHub");
    const field = editor.getByRole("textbox", { name: "标签" });

    fireEvent.input(field, { target: { value: "开发" } });
    fireEvent.keyDown(field, { key: "Enter" });
    fireEvent.input(field, { target: { value: "工作，" } });
    fireEvent.click(editor.getByText("保存"));

    await waitFor(() => expect(screen.getAllByRole("button", { name: "开发" })).toHaveLength(1));
    fireEvent.click(screen.getByRole("button", { name: "工作" }));
    await waitFor(() => expect(titles()).toEqual(["GitHub", "邮箱"]));
    fireEvent.click(screen.getByRole("button", { name: "全部" }));
    await waitFor(() => expect(titles()).toHaveLength(4));
  });

  it("explains a tag that is too long", async () => {
    show();
    const editor = await editRow("GitHub");
    const field = editor.getByRole("textbox", { name: "标签" });

    // The field stops at 30 characters, so a longer tag can only arrive pasted with a comma.
    fireEvent.input(field, { target: { value: `${"长".repeat(31)},` } });
    fireEvent.click(editor.getByText("保存"));

    expect((await screen.findByRole("alert")).textContent).toBe("每个标签不能超过 30 个字符");
  });

  it("shows earlier names and puts one back", async () => {
    show();
    let editor = await editRow("GitHub");
    fireEvent.input(editor.getByLabelText("显示名称"), { target: { value: "GitHub 工作" } });
    fireEvent.click(editor.getByText("保存"));
    await screen.findByText("GitHub 工作");

    editor = await editRow("GitHub 工作");
    const history = editor.getByText("编辑历史").closest("details") as HTMLDetailsElement;
    history.open = true;
    fireEvent(history, new Event("toggle"));
    fireEvent.click(await editor.findByText("用这个"));

    expect((editor.getByLabelText("显示名称") as HTMLInputElement).value).toBe("GitHub");
  });

  it("moves to the trash only after confirming, then restores", async () => {
    show();
    const editor = await editRow("邮箱");

    fireEvent.click(editor.getByText("移入回收站"));
    expect(editor.getByText("移入回收站？30 天内可以恢复")).toBeTruthy();
    fireEvent.click(editor.getAllByText("移入回收站").at(-1) as HTMLElement);

    await waitFor(() => expect(titles()).not.toContain("邮箱"));
    fireEvent.click(await screen.findByText("回收站（1）"));
    const trash = within(await screen.findByRole("dialog"));
    expect(trash.getByText("30 天后彻底删除")).toBeTruthy();
    fireEvent.click(trash.getByText("恢复"));

    await waitFor(() => expect(titles()).toContain("邮箱"));
  });

  it("deletes from the trash for good after a second confirmation", async () => {
    const api = show();
    await api.trash("a1");
    fireEvent.click(await screen.findByText("回收站（1）"));
    const trash = within(await screen.findByRole("dialog"));

    fireEvent.click(await trash.findByText("彻底删除"));
    expect(trash.getByText("彻底删除后无法恢复")).toBeTruthy();
    fireEvent.click(trash.getByRole("button", { name: "彻底删除" }));

    expect(await trash.findByText("回收站是空的")).toBeTruthy();
  });
});

describe("opening an entry from the search panel", () => {
  it("shows every entry and opens the one asked for in its editor", async () => {
    let open: (page: string) => void = () => {};
    const api: MainApi = {
      ...demoMain({ clock: () => STEADY }),
      onPage: (listener) => {
        open = listener;
        return () => {};
      },
    };
    show(api);
    const search = await screen.findByRole("searchbox");
    fireEvent.input(search, { target: { value: "github" } });
    await waitFor(() => expect(screen.getAllByTestId("entry")).toHaveLength(1));
    const steam = (await api.state("steam")).entries[0]?.id as string;

    open(`edit:${steam}`);

    const name = await screen.findByRole("textbox", { name: "显示名称" });
    expect((name as HTMLInputElement).value).toBe("Steam");
    expect((screen.getByRole("searchbox") as HTMLInputElement).value).toBe("");
  });
});
