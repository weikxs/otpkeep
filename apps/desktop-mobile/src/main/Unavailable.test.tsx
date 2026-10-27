import { fireEvent, render, screen, waitFor } from "@testing-library/preact";
import { describe, expect, it, vi } from "vitest";
import type { Problem } from "../panel/api";
import type { MainApi, MainState } from "./api";
import { demoMain } from "./demo";
import { mainStrings } from "./i18n";
import { MainApp } from "./MainApp";

const zh = mainStrings("zh-CN");
const STEADY = 1_700_000_020_000;

/** The demo data, behind a vault that cannot be opened until `fix` runs. */
function stuck(problem: Problem, canRestore = false) {
  const demo = demoMain({ clock: () => STEADY });
  let open = false;
  const fix = vi.fn(async () => {
    open = true;
  });
  const api: MainApi = {
    ...demo,
    state: async (query): Promise<MainState> =>
      open
        ? demo.state(query)
        : { status: "unavailable", problem, canRestore, entries: [], nowMs: STEADY, usesMasterPassword: false, inGroup: false, readOnly: false },
    retryOpen: fix,
    restorePrevious: fix,
  };
  render(<MainApp api={api} text={zh} isMac clock={() => STEADY} />);
  return fix;
}

describe("data that cannot be opened", () => {
  it("opens it on retry once the keychain is back", async () => {
    const fix = stuck("KEYSTORE_UNAVAILABLE");

    fireEvent.click(await screen.findByRole("button", { name: "重试" }));

    expect(await screen.findAllByTestId("entry")).toHaveLength(4);
    expect(fix).toHaveBeenCalledTimes(1);
  });

  it("restores the previous version only after confirming", async () => {
    const fix = stuck("DAMAGED", true);

    fireEvent.click(await screen.findByRole("button", { name: "恢复上一个版本" }));
    expect(fix).not.toHaveBeenCalled();
    expect(screen.getByText(/损坏的文件会保留/u)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "恢复" }));

    expect(await screen.findAllByTestId("entry")).toHaveLength(4);
    expect(fix).toHaveBeenCalledTimes(1);
  });

  it("can step back from restoring", async () => {
    const fix = stuck("DAMAGED", true);

    fireEvent.click(await screen.findByRole("button", { name: "恢复上一个版本" }));
    fireEvent.click(screen.getByRole("button", { name: "取消" }));

    await waitFor(() => expect(screen.getByRole("button", { name: "恢复上一个版本" })).toBeTruthy());
    expect(fix).not.toHaveBeenCalled();
  });

  it("offers no restore without a previous version", async () => {
    stuck("DAMAGED", false);

    expect(await screen.findByText("数据文件已损坏")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "恢复上一个版本" })).toBeNull();
  });
});
