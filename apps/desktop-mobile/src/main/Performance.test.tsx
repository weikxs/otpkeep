import { fireEvent, render, screen, waitFor, within } from "@testing-library/preact";
import { describe, expect, it, vi } from "vitest";
import type { Entry, MainApi } from "./api";
import { demoMain } from "./demo";
import { type EntryRowProps, sameRow } from "./EntryRow";
import { mainStrings } from "./i18n";
import { MainApp } from "./MainApp";

const zh = mainStrings("zh-CN");
// 10 seconds into a 30-second step.
const STEADY = 1_700_000_020_000;
const STEP_MS = 30_000;

function show(api: MainApi, clock: () => number) {
  return render(<MainApp api={api} text={zh} isMac clock={clock} />);
}

describe("the main window's work per change", () => {
  it("saves every changed field of the editor in one call", async () => {
    const api = demoMain({ clock: () => STEADY });
    const update = vi.spyOn(api, "update");
    const singles = [vi.spyOn(api, "edit"), vi.spyOn(api, "rename"), vi.spyOn(api, "setTags"), vi.spyOn(api, "setPinned")];
    show(api, () => STEADY);
    const rows = await screen.findAllByTestId("entry");

    fireEvent.click(within(rows[0] as HTMLElement).getByText("编辑"));
    fireEvent.input(screen.getByLabelText("显示名称"), { target: { value: "GitHub 工作" } });
    fireEvent.input(screen.getByLabelText("备注"), { target: { value: "公司账号" } });
    fireEvent.click(screen.getByLabelText("置顶"));
    fireEvent.input(screen.getByLabelText("账号"), { target: { value: "ops@example.com" } });
    fireEvent.click(screen.getByText("保存"));

    expect(await screen.findByText("GitHub 工作")).toBeTruthy();
    expect(update).toHaveBeenCalledTimes(1);
    expect(update).toHaveBeenCalledWith("a1", { title: "GitHub 工作", note: "公司账号", account: "ops@example.com", pinned: true });
    for (const single of singles) expect(single).not.toHaveBeenCalled();
  });

  it("refreshes codes when they roll over without asking for the trash again", async () => {
    let now = STEADY;
    const clock = () => now;
    const api = demoMain({ clock });
    const state = vi.spyOn(api, "state");
    const trashList = vi.spyOn(api, "trashList");
    show(api, clock);
    await screen.findAllByTestId("entry");
    await waitFor(() => expect(trashList).toHaveBeenCalled());
    const [states, trashes] = [state.mock.calls.length, trashList.mock.calls.length];

    now = STEADY + STEP_MS;

    await waitFor(() => expect(state.mock.calls.length).toBeGreaterThan(states));
    expect(trashList).toHaveBeenCalledTimes(trashes);
  });
});

describe("a row between two ticks", () => {
  const entry: Entry = {
    id: "a1",
    title: "GitHub",
    issuer: "GitHub",
    account: "demo-user",
    note: "",
    tags: [],
    kind: "totp",
    pinned: false,
    code: "123456",
    periodS: 30,
    validUntilMs: STEADY + 10_000,
  };
  const props = (changes: Partial<EntryRowProps>): EntryRowProps =>
    ({
      entry,
      coreNow: STEADY,
      copied: false,
      text: zh,
      isMac: true,
      previous: undefined,
      next: "a2",
      sortable: true,
      onCopy: () => undefined,
      ...changes,
    }) as EntryRowProps;

  it("is drawn again only when its countdown shows another second", () => {
    expect(sameRow(props({}), props({ coreNow: STEADY + 400 }))).toBe(true);
    expect(sameRow(props({}), props({ coreNow: STEADY + 1_000 }))).toBe(false);
  });

  it("is drawn again when its own data or place changes", () => {
    expect(sameRow(props({}), props({ entry: { ...entry, title: "GitHub 工作" } }))).toBe(false);
    expect(sameRow(props({}), props({ copied: true }))).toBe(false);
    expect(sameRow(props({}), props({ next: undefined }))).toBe(false);
    expect(sameRow(props({}), props({ selection: { chosen: true, onToggle: () => undefined } }))).toBe(false);
  });

  it("ignores new handlers, which always reach the window's current state", () => {
    expect(sameRow(props({}), props({ onCopy: () => undefined }))).toBe(true);
  });
});
