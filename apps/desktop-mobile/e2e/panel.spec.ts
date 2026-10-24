import { expect, test } from "@playwright/test";

const SHOTS = process.env.PANEL_SHOTS;

test.use({ locale: "zh-CN" });

for (const scheme of ["light", "dark"] as const) {
  test(`the menu bar panel loads in ${scheme} mode without errors`, async ({ page }) => {
    const problems: string[] = [];
    page.on("pageerror", (error) => problems.push(error.message));
    page.on("console", (message) => {
      if (message.type() === "error") problems.push(message.text());
    });
    await page.setViewportSize({ width: 340, height: 480 });
    await page.emulateMedia({ colorScheme: scheme });

    await page.goto("/panel.html");

    await expect(page.getByTestId("entry")).toHaveCount(6);
    await page.getByTestId("entry").nth(1).hover();
    if (SHOTS) await page.screenshot({ path: `${SHOTS}/panel-${scheme}.png` });
    await page.getByTestId("entry").first().click();
    await expect(page.getByText("已复制 ✓")).toBeVisible();
    await expect(page.getByRole("status")).toHaveText("已复制 · 30 秒后从剪贴板清除");
    if (SHOTS) await page.screenshot({ path: `${SHOTS}/panel-${scheme}-copied.png` });
    expect(problems).toEqual([]);
  });
}
