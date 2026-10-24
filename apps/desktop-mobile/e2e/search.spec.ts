import { expect, test } from "@playwright/test";

const SHOTS = process.env.PANEL_SHOTS;

test.use({ locale: "zh-CN" });

for (const scheme of ["light", "dark"] as const) {
  test(`the search panel finds an account in ${scheme} mode without errors`, async ({ page }) => {
    const problems: string[] = [];
    page.on("pageerror", (error) => problems.push(error.message));
    page.on("console", (message) => {
      if (message.type() === "error") problems.push(message.text());
    });
    await page.setViewportSize({ width: 620, height: 400 });
    await page.emulateMedia({ colorScheme: scheme });

    await page.goto("/search.html");

    await expect(page.getByTestId("entry")).toHaveCount(6);
    await expect(page.getByRole("searchbox")).toBeFocused();
    await page.keyboard.type("example");
    await page.keyboard.press("ArrowDown");
    await expect(page.locator(".entry.selected")).toContainText("Cloud Console");
    if (SHOTS) await page.screenshot({ path: `${SHOTS}/search-${scheme}.png` });
    await page.keyboard.press("Shift+Enter");
    await expect(page.getByRole("status")).toContainText("已改为复制");
    if (SHOTS) await page.screenshot({ path: `${SHOTS}/search-${scheme}-fallback.png` });
    await page.keyboard.press("Escape");
    await expect(page.getByRole("searchbox")).toHaveValue("");
    expect(problems).toEqual([]);
  });
}
