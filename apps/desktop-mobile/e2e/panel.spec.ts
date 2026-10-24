import { expect, test } from "@playwright/test";

const SHOTS = process.env.PANEL_SHOTS;

test.use({ locale: "zh-CN", permissions: ["clipboard-read", "clipboard-write"] });

for (const scheme of ["light", "dark"] as const) {
  test(`the menu bar panel loads in ${scheme} mode without errors`, async ({ page }) => {
    const problems: string[] = [];
    page.on("pageerror", (error) => problems.push(error.message));
    page.on("console", (message) => {
      if (message.type() === "error") problems.push(message.text());
    });
    await page.setViewportSize({ width: 340, height: 560 });
    await page.emulateMedia({ colorScheme: scheme });

    await page.goto("/panel.html");

    await expect(page.getByTestId("entry")).toHaveCount(6);
    await page.getByTestId("entry").nth(1).hover();
    if (SHOTS) await page.screenshot({ path: `${SHOTS}/panel-${scheme}.png` });
    await page.evaluate(() => navigator.clipboard.writeText(""));
    await page.getByTestId("entry").first().click();
    await expect(page.getByText("已复制 ✓")).toBeVisible();
    // The preview really copies: a demo code is on the clipboard (the code may roll over meanwhile).
    expect(await page.evaluate(() => navigator.clipboard.readText())).toMatch(/^\d{6}$/u);
    await expect(page.getByRole("status")).toHaveText("已复制 · 30 秒后从剪贴板清除");
    if (SHOTS) await page.screenshot({ path: `${SHOTS}/panel-${scheme}-copied.png` });
    expect(problems).toEqual([]);
  });
}
