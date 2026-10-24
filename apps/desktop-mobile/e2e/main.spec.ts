import { expect, test } from "@playwright/test";

const SHOTS = process.env.PANEL_SHOTS;

test.use({ locale: "zh-CN" });

for (const scheme of ["light", "dark"] as const) {
  test(`the main window lists accounts and adds a pasted link (${scheme})`, async ({ page }) => {
    const problems: string[] = [];
    page.on("pageerror", (error) => problems.push(error.message));
    page.on("console", (message) => {
      if (message.type() === "error") problems.push(message.text());
    });
    await page.emulateMedia({ colorScheme: scheme });
    await page.setViewportSize({ width: 900, height: 640 });

    await page.goto("/index.html");

    await expect(page.getByTestId("entry")).toHaveCount(4);
    if (SHOTS) await page.screenshot({ path: `${SHOTS}/main-${scheme}.png` });

    await page.getByText("+ 添加").click();
    const dialog = page.getByRole("dialog");
    await dialog.getByLabel("粘贴链接").fill(
      "请扫描：otpauth://totp/Example%20Cloud:ops@example.com?secret=JBSWY3DPEHPK3PXP\nphonefactor://activate_account?code=1",
    );
    await dialog.getByText("读取").click();
    await expect(dialog.getByText("将添加 1 个账号")).toBeVisible();
    await dialog.getByText("1 处无法读取").click();
    if (SHOTS) await page.screenshot({ path: `${SHOTS}/main-${scheme}-preview.png` });

    await dialog.getByRole("button", { name: "添加 1 个" }).click();
    await expect(page.getByTestId("entry")).toHaveCount(5);
    expect(problems).toEqual([]);
  });
}

test("the manual form explains a mistyped secret", async ({ page }) => {
  await page.setViewportSize({ width: 900, height: 640 });
  await page.goto("/index.html");
  await page.getByText("+ 添加").click();
  const dialog = page.getByRole("dialog");

  await dialog.getByRole("tab", { name: "手动输入" }).click();
  await dialog.getByLabel("服务商").fill("Example");
  await dialog.getByLabel("密钥").fill("JBSW 0Y3D");
  await dialog.getByText("高级设置").click();
  if (SHOTS) await page.screenshot({ path: `${SHOTS}/main-manual.png` });
  await dialog.getByText("预览").click();

  await expect(dialog.getByRole("alert")).toContainText("第 5 个字符“0”不合法");
});
