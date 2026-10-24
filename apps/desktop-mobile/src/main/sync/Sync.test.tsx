import { fireEvent, render, screen, waitFor, within } from "@testing-library/preact";
import { describe, expect, it, vi } from "vitest";
import { demoMain } from "../demo";
import { mainStrings } from "../i18n";
import { MainApp } from "../MainApp";

const zh = mainStrings("zh-CN");
const STEADY = 1_700_000_020_000;

async function openSync(options: { inGroup?: boolean } = {}) {
  const api = demoMain({ clock: () => STEADY, ...options });
  render(<MainApp api={api} text={zh} isMac clock={() => STEADY} />);
  fireEvent.click(await screen.findByRole("button", { name: "同步" }));
  return { api, page: within(await screen.findByRole("dialog", { name: "同步" })) };
}

describe("sync page", () => {
  it("starts a group by adding a device, and shows the pairing code", async () => {
    const { page } = await openSync();
    expect(await page.findByText("还没有加入同步组")).toBeTruthy();

    fireEvent.input(page.getByLabelText("本机名称"), { target: { value: "办公室 Mac" } });
    fireEvent.click(page.getByText("本机作为同步组的第一台设备，再把其他设备加进来"));

    expect((await page.findByTestId("window-code")).textContent).toBe("7391 5206");
    expect(page.getByText("未使用蓝牙：蓝牙未开启")).toBeTruthy();
    fireEvent.click(page.getByText("显示二维码"));
    expect(((await page.findByAltText("显示二维码")) as HTMLImageElement).src).toMatch(/^data:image\/svg\+xml/u);
  });

  it("lists the devices with when they last synced", async () => {
    const { page } = await openSync({ inGroup: true });

    const devices = await page.findAllByTestId("device");
    expect(devices).toHaveLength(2);
    expect(devices[1]?.textContent).toContain("3 天前同步");
    expect(page.getByText("AB12-CD34-EF56-7890")).toBeTruthy();
  });

  it("waits for a sync, asks to allow the request and reports the result", async () => {
    const { api, page } = await openSync({ inGroup: true });

    fireEvent.click(await page.findByText("等待同步"));
    expect((await page.findByTestId("window-code")).textContent).toBe("482 913");
    expect(page.getByText(/后关闭$/u)).toBeTruthy();
    api.request("我的手机");

    const ask = within(await page.findByRole("alertdialog", { name: "请求同步" }));
    expect(ask.getByText("我的手机 · 9F8E-7D6C-5B4A-3210（iOS）请求与本机同步")).toBeTruthy();
    fireEvent.click(ask.getByText("允许"));

    expect((await page.findByRole("status")).textContent).toBe("已与 我的手机 同步，本机现在有 4 个账号");
  });

  it("says from this side when the person here declined", async () => {
    const { api, page } = await openSync({ inGroup: true });
    fireEvent.click(await page.findByText("等待同步"));
    await page.findByTestId("window-code");
    api.request("陌生设备");

    fireEvent.click(within(await page.findByRole("alertdialog")).getByText("拒绝"));

    expect((await page.findByRole("status")).textContent).toBe("你拒绝了这个同步请求，本次未生效");
  });

  it("syncs with a device found nearby by entering its code", async () => {
    const { page } = await openSync({ inGroup: true });

    fireEvent.click(await page.findByText("搜索附近正在等待的设备"));
    const code = await page.findByLabelText("输入 我的手机 上显示的 6 位同步码");
    expect(page.getByText("9F8E · 蓝牙")).toBeTruthy();
    fireEvent.input(code, { target: { value: "482 913" } });
    fireEvent.click(page.getByText("连接"));

    expect((await page.findByRole("status")).textContent).toBe("已与 我的手机 同步，本机现在有 4 个账号");
  });

  it("explains a wrong code", async () => {
    const { page } = await openSync({ inGroup: true });
    fireEvent.click(await page.findByText("搜索附近正在等待的设备"));
    fireEvent.input(await page.findByLabelText(/6 位同步码/u), { target: { value: "000000" } });

    fireEvent.click(page.getByText("连接"));

    expect((await page.findByRole("status")).textContent).toBe("码不正确，请核对后重试");
  });

  it("joins a group, compares the check code and reports the pairing", async () => {
    const { api, page } = await openSync();
    fireEvent.click(await page.findByText("加入已有的同步组"));
    fireEvent.input(await page.findByLabelText("输入 我的手机 上显示的 8 位配对码"), { target: { value: "7391 5206" } });
    expect(page.getByText("本机已有账号，加入后：")).toBeTruthy();
    fireEvent.click(page.getByText("用同步组的账号替换（本机账号先备份）"));
    const join = vi.spyOn(api, "joinPair");

    fireEvent.click(page.getByText("连接"));
    await waitFor(() => expect(join).toHaveBeenCalledWith({ by: "found", index: 0, code: "7391 5206" }, null, true));
    expect(await page.findByText("正在连接 我的手机…")).toBeTruthy();
    api.checkCode();
    expect((await page.findByTestId("sas")).textContent).toBe("305 118");
    fireEvent.click(page.getByText("一致"));

    expect((await page.findByRole("status")).textContent).toBe("已与 我的手机 配对，两台设备现在同步");
    expect(await page.findAllByTestId("device")).toHaveLength(2);
  });

  it("renames this device", async () => {
    const { page } = await openSync({ inGroup: true });

    fireEvent.click(await page.findByText("改名"));
    fireEvent.input(page.getByLabelText("本机名称"), { target: { value: "家里的 Mac" } });
    fireEvent.click(page.getByRole("button", { name: "改名" }));

    // This device's name heads the page and leads the devices.
    expect(await page.findAllByText("家里的 Mac")).toHaveLength(2);
  });

  it("removes a device and leaves the group", async () => {
    const { page } = await openSync({ inGroup: true });

    fireEvent.click(await page.findByText("移除"));
    await waitFor(() => expect(page.getAllByTestId("device")).toHaveLength(1));
    fireEvent.click(page.getByText("离开同步组"));

    expect(await page.findByText("还没有加入同步组")).toBeTruthy();
  });

  it("cancels a window", async () => {
    const { page } = await openSync({ inGroup: true });
    fireEvent.click(await page.findByText("等待同步"));
    await page.findByTestId("window-code");

    fireEvent.click(page.getByRole("button", { name: "取消" }));

    expect((await page.findByRole("status")).textContent).toBe("已取消，本次未生效");
  });
});
