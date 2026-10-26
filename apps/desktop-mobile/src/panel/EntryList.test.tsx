import { fireEvent, render, screen, waitFor } from "@testing-library/preact";
import { describe, expect, it, vi } from "vitest";
import type { PanelEntry, PanelState } from "./api";
import { demoApi } from "./demo";
import { EntryList, PasswordPrompt, generateIfNeeded, samePanelRow } from "./EntryList";
import { strings } from "./i18n";
import { Unavailable } from "./Unavailable";

const zh = strings("zh-CN");
// 10 seconds into a 30-second step: no "next" code yet.
const STEADY = 1_700_000_020_000;
// 3 seconds before the step ends.
const ENDING = 1_700_000_037_000;

async function entriesAt(at: number): Promise<PanelEntry[]> {
  return (await demoApi({ clock: () => at }).state("")).entries;
}

function list(entries: PanelEntry[], at = STEADY, onCopy = vi.fn(), query = "") {
  return render(
    <EntryList
      entries={entries}
      query={query}
      selected={0}
      copiedId={null}
      coreNow={at}
      epoch={1}
      text={zh}
      isMac
      onCopy={onCopy}
      onHover={() => undefined}
    />,
  );
}

describe("the account list", () => {
  it("lists pinned entries first under their own heading", async () => {
    list(await entriesAt(STEADY));

    expect(screen.getByText("置顶")).toBeTruthy();
    expect(screen.getByText("全部")).toBeTruthy();
    expect(screen.getAllByTestId("entry")[0]?.textContent).toContain("GitHub");
  });

  it("drops the group headings while searching", async () => {
    list(await entriesAt(STEADY), STEADY, vi.fn(), "vpn");

    expect(screen.queryByText("置顶")).toBeNull();
  });

  it("shows codes in groups of three", async () => {
    list(await entriesAt(STEADY));

    expect(screen.getAllByTestId("code")[0]?.textContent).toMatch(/^\d{3} \d{3}$/u);
  });

  it("shows the next code when the current one is ending", async () => {
    list(await entriesAt(ENDING), ENDING);

    expect(screen.getAllByText(/下一个/u).length).toBeGreaterThan(0);
  });

  it("shows each row's shortcut the way the Mac writes it", async () => {
    list(await entriesAt(STEADY));

    expect(screen.getAllByTestId("entry")[0]?.textContent).toContain("⌃⌥⌘1");
  });

  it("copies the chosen row", async () => {
    const onCopy = vi.fn();
    const entries = await entriesAt(STEADY);
    list(entries, STEADY, onCopy);

    fireEvent.click(screen.getAllByTestId("entry")[1] as HTMLElement);

    expect(onCopy).toHaveBeenCalledWith(entries[1]);
  });

  it("generates a counter code only when one is needed", async () => {
    const api = demoApi({ clock: () => STEADY });
    const counter = (await api.state("")).entries.find((e) => e.kind === "hotp") as PanelEntry;
    const nextHotp = vi.spyOn(api, "nextHotp");
    let state: PanelState | null = await api.state("");
    const setState = (update: (current: PanelState | null) => PanelState | null) => {
      state = update(state);
    };

    await generateIfNeeded(api, counter, setState);
    const generated = state?.entries.find((e) => e.id === counter.id) as PanelEntry;
    await generateIfNeeded(api, generated, setState);

    expect(generated.code).toMatch(/^\d{6}$/u);
    expect(nextHotp).toHaveBeenCalledOnce();
  });
});

describe("the master password prompt", () => {
  it("rejects a wrong password and opens with the right one", async () => {
    const onOpened = vi.fn();
    render(<PasswordPrompt api={demoApi({ status: "needsPassword" })} text={zh} onOpened={onOpened} />);
    const password = screen.getByLabelText("主口令");

    fireEvent.input(password, { target: { value: "nope" } });
    fireEvent.click(screen.getByText("打开"));
    expect(await screen.findByText("主口令不正确")).toBeTruthy();

    fireEvent.input(password, { target: { value: "demo" } });
    fireEvent.click(screen.getByText("打开"));
    await waitFor(() => expect(onOpened).toHaveBeenCalledOnce());
    expect(onOpened.mock.calls[0]?.[0].status).toBe("ready");
  });

  it("asks for the password instead of calling an empty one wrong", async () => {
    const api = demoApi({ status: "needsPassword" });
    const unlock = vi.spyOn(api, "unlock");
    render(<PasswordPrompt api={api} text={zh} onOpened={() => undefined} />);

    fireEvent.click(screen.getByRole("button", { name: "打开" }));

    expect((await screen.findByRole("alert")).textContent).toBe("请输入主口令");
    expect(unlock).not.toHaveBeenCalled();
  });

  it("keeps the password field away from AutoFill", () => {
    render(<PasswordPrompt api={demoApi({ status: "needsPassword" })} text={zh} onOpened={() => undefined} />);

    expect(screen.getByLabelText("主口令").getAttribute("autocomplete")).toBe("off");
  });
});

describe("data that cannot be opened", () => {
  const unavailable = async (problem: "BUSY" | "KEY_MISSING") =>
    ({ ...(await demoApi({ status: "unavailable", problem }).state("")), problem }) as PanelState;

  it("tries again for a problem that can pass", async () => {
    const onRetry = vi.fn(async () => {});
    const onOpenMain = vi.fn();
    render(<Unavailable state={await unavailable("BUSY")} text={zh} onRetry={onRetry} onOpenMain={onOpenMain} />);

    fireEvent.click(screen.getByRole("button", { name: "重试" }));
    fireEvent.click(screen.getByText("在主窗口中处理"));

    await waitFor(() => expect(onRetry).toHaveBeenCalledOnce());
    expect(onOpenMain).toHaveBeenCalledOnce();
  });

  it("offers no retry for a problem that trying again cannot fix", async () => {
    render(<Unavailable state={await unavailable("KEY_MISSING")} text={zh} onRetry={async () => {}} onOpenMain={() => {}} />);

    expect(screen.getByText("本机找不到这份数据的密钥")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "重试" })).toBeNull();
  });
});

describe("strings", () => {
  it("falls back to English", () => {
    expect(strings("en-US").unlock).toBe("Open");
    expect(strings("fr").copiedToast(0)).toBe("Copied");
  });
});

describe("copying in a browser preview", () => {
  it("really puts the demo code on the clipboard", async () => {
    const api = demoApi({ clock: () => STEADY });
    const first = (await api.state("")).entries[0] as PanelEntry;

    await api.copy(first.id);

    expect(await navigator.clipboard.readText()).toBe(first.code);
  });

  it("fails instead of pretending, when the browser refuses", async () => {
    const write = vi.spyOn(navigator.clipboard, "writeText").mockRejectedValueOnce(new Error("denied"));
    const api = demoApi({ clock: () => STEADY });

    await expect(api.copy("d1")).rejects.toThrow();
    write.mockRestore();
  });
});

describe("a row between two ticks", () => {
  const entry = { id: "d1", title: "GitHub", issuer: "GitHub", account: "demo-user", note: "", tags: [], kind: "totp", pinned: true, code: "123456", periodS: 30, validUntilMs: STEADY + 10_000 } as const;
  const onCopy = () => undefined;
  const onHover = () => undefined;
  const props = (changes: Partial<Parameters<typeof samePanelRow>[0]> = {}): Parameters<typeof samePanelRow>[0] => ({
    entry: { ...entry, tags: [] },
    index: 0,
    position: 1,
    size: 6,
    selected: false,
    copied: false,
    coreNow: STEADY,
    epoch: 1,
    text: zh,
    isMac: true,
    onCopy,
    onHover,
    ...changes,
  });

  it("is drawn again only when its countdown shows another second", () => {
    const base = props();
    expect(samePanelRow(base, { ...base, coreNow: STEADY + 400 })).toBe(true);
    expect(samePanelRow(base, { ...base, coreNow: STEADY + 1_000 })).toBe(false);
  });

  it("is drawn again when it is chosen, copied or its codes are loaded again", () => {
    const base = props();
    expect(samePanelRow(base, { ...base, selected: true })).toBe(false);
    expect(samePanelRow(base, { ...base, copied: true })).toBe(false);
    expect(samePanelRow(base, { ...base, epoch: 2 })).toBe(false);
  });
});
