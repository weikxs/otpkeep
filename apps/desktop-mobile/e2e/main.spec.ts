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

test("rows are reordered by dragging in a real browser", async ({ page }) => {
  await page.setViewportSize({ width: 900, height: 640 });
  await page.goto("/index.html");
  const rows = page.getByTestId("entry");
  await expect(rows).toHaveCount(4);

  const target = await rows.nth(0).boundingBox();
  if (!target) throw new Error("no layout");
  await rows.nth(3).dragTo(rows.nth(0), { targetPosition: { x: 40, y: 4 } });

  await expect(rows.nth(0)).toContainText("Bank token");
});

test("the editor shows tags, place, history and the trash", async ({ page }) => {
  await page.setViewportSize({ width: 900, height: 760 });
  await page.goto("/index.html");
  const mail = page.getByTestId("entry").nth(1);

  await mail.getByText("编辑").click();
  await page.getByRole("textbox", { name: "标签" }).fill("个人");
  await page.getByRole("textbox", { name: "标签" }).press("Enter");
  await page.getByText("编辑历史").click();
  if (SHOTS) await page.screenshot({ path: `${SHOTS}/main-editor.png` });
  await page.getByText("移入回收站").click();
  await page.getByRole("button", { name: "移入回收站" }).last().click();

  await expect(page.getByTestId("entry")).toHaveCount(3);
  await page.getByText("回收站（1）").click();
  await expect(page.getByTestId("trash-row")).toHaveCount(1);
  if (SHOTS) await page.screenshot({ path: `${SHOTS}/main-trash.png` });
});

test("the advanced part shows the shortcut and code settings", async ({ page }) => {
  await page.setViewportSize({ width: 900, height: 900 });
  await page.goto("/index.html");

  await page.getByTestId("entry").nth(0).getByText("编辑").click();
  await page.getByText("服务商、账号、验证码设置与快捷键").click();
  await page.getByText("修改").click();
  await page.getByLabel("新密钥").fill("JBSW Y3DP");
  await page.getByRole("button", { name: "预览" }).click();

  await expect(page.getByText(/按新设置，现在的验证码是/u)).toBeVisible();
  if (SHOTS) await page.screenshot({ path: `${SHOTS}/main-advanced.png`, fullPage: true });
});

test("several accounts are tagged at once", async ({ page }) => {
  await page.setViewportSize({ width: 900, height: 640 });
  await page.goto("/index.html");

  await page.getByText("选择", { exact: true }).click();
  await page.getByRole("checkbox", { name: "GitHub" }).check();
  await page.getByRole("checkbox", { name: "Steam" }).check();
  await page.getByText("加标签").click();
  await page.getByLabel("要添加的标签").fill("常用");
  if (SHOTS) await page.screenshot({ path: `${SHOTS}/main-batch.png` });
  await page.getByText("应用").click();

  await expect(page.getByText("已修改 2 个账号")).toBeVisible();
});

test("the settings switch the theme for the whole window", async ({ page }) => {
  await page.emulateMedia({ colorScheme: "light" });
  await page.setViewportSize({ width: 900, height: 640 });
  await page.goto("/index.html");

  await page.getByRole("button", { name: "设置" }).click();
  await page.getByLabel("主题").selectOption("dark");

  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  const background = await page.evaluate(() => getComputedStyle(document.body).backgroundColor);
  expect(background).toBe("rgb(22, 23, 26)");
  if (SHOTS) await page.screenshot({ path: `${SHOTS}/main-settings.png` });
});

test("the export dialog offers the formats and shows Google codes", async ({ page }) => {
  await page.setViewportSize({ width: 900, height: 760 });
  await page.goto("/index.html");

  await page.getByRole("button", { name: "导出", exact: true }).click();
  if (SHOTS) await page.screenshot({ path: `${SHOTS}/main-export.png` });
  await page.getByText("Google 验证器转移码").click();
  await page.getByText("显示二维码…").click();

  await expect(page.getByText("第 1 / 2 张")).toBeVisible();
  if (SHOTS) await page.screenshot({ path: `${SHOTS}/main-google.png` });
});
