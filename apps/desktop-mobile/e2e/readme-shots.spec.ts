import { type Page, test } from "@playwright/test";

/**
 * The screenshots in the README, from demo data only. Skipped unless README_SHOTS names the folder to
 * write to (docs/images); each language gets its own subfolder.
 *
 *   README_SHOTS=../../docs/images pnpm e2e readme-shots   (from apps/desktop-mobile)
 */
const OUT = process.env.README_SHOTS;

const LOCALES = {
  en: { locale: "en-US", add: "+ Add", paste: "Paste a link", read: "Read", settings: "Settings", sync: "Sync" },
  zh: { locale: "zh-CN", add: "+ 添加", paste: "粘贴链接", read: "读取", settings: "设置", sync: "同步" },
} as const;

const LINKS =
  "otpauth://totp/Example%20Cloud:ops@example.com?secret=JBSWY3DPEHPK3PXP\n" +
  "otpauth://totp/Example%20Shop:demo@example.com?secret=GEZDGNBVGY3TQOJQ";

/** Stands in for OTPKeep behind the management page, with made-up accounts and codes. */
async function fakeGate(page: Page, note: string) {
  const entries = [
    { id: "e1", title: "GitHub", issuer: "GitHub", account: "demo-user", note: "", pinned: true, kind: "totp", defaultShortcut: "control+alt+super+Digit1" },
    { id: "e2", title: "Example Mail", issuer: "Example Mail", account: "demo@example.com", note, pinned: false, kind: "totp", shortcut: "control+alt+super+KeyM", defaultShortcut: "control+alt+super+Digit2" },
    { id: "e3", title: "Example Cloud", issuer: "Example Cloud", account: "ops@example.com", note: "", pinned: false, kind: "totp", defaultShortcut: "control+alt+super+Digit3" },
    { id: "e4", title: "Example Bank", issuer: "Example Bank", account: "0000", note: "", pinned: false, kind: "hotp", defaultShortcut: "control+alt+super+Digit4" },
  ];
  await page.route("**/api/**", async (route) => {
    const url = new URL(route.request().url());
    if (url.pathname === "/api/session") return route.fulfill({ json: { session: "s1" } });
    if (url.pathname === "/api/entries") {
      const nowMs = Date.now();
      const codes = ["492 081", "730 516", "164 923"];
      const shown = entries.map((e, i) =>
        e.kind === "totp"
          ? { ...e, code: codes[i]?.replace(" ", ""), periodS: 30, validUntilMs: nowMs + 18_000 }
          : e,
      );
      return route.fulfill({ json: { entries: shown, nowMs } });
    }
    return route.fulfill({ json: {} });
  });
}

/** The quick panel's window is transparent; a plain backdrop stands in for the desktop. */
async function backdrop(page: Page, scheme: "light" | "dark") {
  await page.addStyleTag({ content: `html { background: ${scheme === "light" ? "#dcd9d0" : "#2a2c31"} !important; }` });
}

for (const [lang, words] of Object.entries(LOCALES)) {
  test.describe(`README screenshots (${lang})`, () => {
    test.skip(!OUT, "set README_SHOTS to write the README's screenshots");
    test.use({ locale: words.locale, deviceScaleFactor: 2 });
    const dir = `${OUT}/${lang}`;

    test("main window", async ({ page }) => {
      for (const scheme of ["light", "dark"] as const) {
        await page.emulateMedia({ colorScheme: scheme });
        await page.setViewportSize({ width: 1100, height: 520 });
        await page.goto("/index.html");
        await page.getByTestId("entry").first().waitFor();
        await page.screenshot({ path: `${dir}/main-${scheme}.png` });
      }
    });

    test("adding accounts", async ({ page }) => {
      await page.setViewportSize({ width: 1100, height: 720 });
      await page.goto("/index.html");
      await page.getByText(words.add).click();
      const dialog = page.getByRole("dialog");
      await dialog.getByLabel(words.paste).fill(LINKS);
      await dialog.getByText(words.read, { exact: true }).click();
      await dialog.getByTestId("preview-row").first().waitFor().catch(() => page.waitForTimeout(400));
      await page.screenshot({ path: `${dir}/add.png` });
    });

    test("settings", async ({ page }) => {
      await page.setViewportSize({ width: 1100, height: 860 });
      await page.goto("/index.html");
      await page.getByRole("button", { name: words.settings, exact: true }).click();
      await page.getByRole("dialog").waitFor();
      await page.waitForTimeout(300);
      await page.screenshot({ path: `${dir}/settings.png` });
    });

    test("sync", async ({ page }) => {
      await page.setViewportSize({ width: 1100, height: 720 });
      await page.goto("/index.html");
      await page.getByRole("button", { name: words.sync, exact: true }).click();
      await page.getByRole("dialog").waitFor();
      await page.waitForTimeout(300);
      await page.screenshot({ path: `${dir}/sync.png` });
    });

    test("quick panel", async ({ page }) => {
      for (const scheme of ["light", "dark"] as const) {
        await page.emulateMedia({ colorScheme: scheme });
        await page.setViewportSize({ width: 340, height: 560 });
        await page.goto("/panel.html");
        await backdrop(page, scheme);
        await page.getByTestId("entry").first().waitFor();
        await page.screenshot({ path: `${dir}/panel-${scheme}.png` });
      }
    });

    test("search panel", async ({ page }) => {
      await page.setViewportSize({ width: 620, height: 400 });
      await page.goto("/search.html");
      await page.getByTestId("entry").first().waitFor();
      await page.keyboard.type("example");
      await page.keyboard.press("ArrowDown");
      await page.waitForTimeout(200);
      await page.screenshot({ path: `${dir}/search.png` });
    });

    test("management page", async ({ page }) => {
      await page.setViewportSize({ width: 1000, height: 420 });
      await fakeGate(page, lang === "zh" ? "备用码在抽屉里" : "Backup codes in the drawer");
      await page.goto("/gate.html#k=demo");
      await page.getByTestId("card").first().waitFor();
      await page.screenshot({ path: `${dir}/gate.png` });
    });
  });
}
