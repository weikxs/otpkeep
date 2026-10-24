import { expect, test } from "@playwright/test";

test("the main window page loads without errors", async ({ page }) => {
  const problems: string[] = [];
  page.on("pageerror", (error) => problems.push(error.message));
  page.on("console", (message) => {
    if (message.type() === "error") problems.push(message.text());
  });

  await page.goto("/index.html");

  await expect(page.getByRole("heading", { level: 1 })).toHaveText("OTPKeep");
  expect(problems).toEqual([]);
});
