import { expect, test } from "@playwright/test";

test("the browser management page loads without errors", async ({ page }) => {
  const problems: string[] = [];
  page.on("pageerror", (error) => problems.push(error.message));
  page.on("console", (message) => {
    if (message.type() === "error") problems.push(message.text());
  });

  await page.goto("/gate.html");

  await expect(page.getByRole("heading", { level: 1 })).toHaveText("OTPKeep");
  await expect(page.getByTestId("surface")).toHaveText("Browser management page");
  expect(problems).toEqual([]);
});
