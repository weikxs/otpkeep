import { expect, test } from "@playwright/test";

test.use({ locale: "zh-CN" });

for (const scheme of ["light", "dark"] as const) {
  test(`the password window loads in ${scheme} mode without errors`, async ({ page }) => {
    const problems: string[] = [];
    page.on("pageerror", (error) => problems.push(error.message));
    page.on("console", (message) => {
      if (message.type() === "error") problems.push(message.text());
    });
    await page.setViewportSize({ width: 380, height: 240 });
    await page.emulateMedia({ colorScheme: scheme });

    await page.goto("/unlock.html");

    const password = page.getByLabel("主口令");
    await expect(password).toBeFocused();
    await password.fill("nope");
    await page.getByRole("button", { name: "打开" }).click();
    await expect(page.getByRole("alert")).toHaveText("主口令不正确");
    await expect(password).toHaveValue("");
    expect(problems).toEqual([]);
  });
}
