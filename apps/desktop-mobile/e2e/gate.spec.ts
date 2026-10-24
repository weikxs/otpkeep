import { type Page, expect, test } from "@playwright/test";

const SHOTS = process.env.PANEL_SHOTS;

test.use({ locale: "zh-CN" });

interface Entry {
  id: string;
  title: string;
  issuer: string;
  account: string;
  note: string;
  pinned: boolean;
  kind: string;
  shortcut?: string;
  defaultShortcut?: string;
}

/** Stands in for OTPKeep: the same routes and rules, made-up accounts. */
async function fakeOtpkeep(page: Page) {
  const entries: Entry[] = [
    { id: "e1", title: "GitHub", issuer: "GitHub", account: "demo-user", note: "", pinned: true, kind: "totp", defaultShortcut: "control+alt+super+Digit1" },
    { id: "e2", title: "邮箱", issuer: "示例邮箱", account: "demo@example.com", note: "备用码在抽屉里", pinned: false, kind: "totp", shortcut: "control+alt+super+KeyM", defaultShortcut: "control+alt+super+Digit2" },
    { id: "e3", title: "Example Cloud", issuer: "Example Cloud", account: "ops@example.com", note: "", pinned: false, kind: "totp", defaultShortcut: "control+alt+super+Digit3" },
  ];
  const seen = { sessions: 0, bearers: new Set<string>(), copied: [] as string[] };
  await page.route("**/api/**", async (route) => {
    const request = route.request();
    const url = new URL(request.url());
    if (url.pathname === "/api/session") {
      seen.sessions += 1;
      const ok = seen.sessions === 1 && JSON.parse(request.postData() ?? "{}").link === "one-time";
      return route.fulfill(ok ? { json: { session: "s1" } } : { status: 401, json: { error: { code: "LINK_USED" } } });
    }
    seen.bearers.add(request.headers().authorization ?? "");
    if (url.pathname === "/api/entries") {
      const nowMs = Date.now();
      const withCodes = entries.map((e, i) => ({ ...e, code: `${i + 1}${i + 1}${i + 1}456`, periodS: 30, validUntilMs: nowMs + 20_000 }));
      return route.fulfill({ json: { entries: withCodes, nowMs } });
    }
    // OTPKeep writes the clipboard itself; the page only learns when it clears.
    if (url.pathname.endsWith("/copy")) {
      seen.copied.push(url.pathname.split("/")[3] ?? "");
      return route.fulfill({ json: { clearsInS: 30 } });
    }
    const match = /^\/api\/entries\/([^/]+)(\/shortcut)?$/u.exec(url.pathname);
    const entry = entries.find((e) => e.id === match?.[1]);
    if (!entry) return route.fulfill({ status: 404, json: { error: { code: "NOT_FOUND" } } });
    const body = JSON.parse(request.postData() ?? "{}");
    if (match?.[2]) entry.shortcut = (body.shortcut as string).toLowerCase().replace("keyp", "KeyP");
    else Object.assign(entry, body);
    return route.fulfill({ json: { entry } });
  });
  return seen;
}

for (const scheme of ["light", "dark"] as const) {
  test(`the management page edits a note and sets a shortcut (${scheme})`, async ({ page }) => {
    const problems: string[] = [];
    page.on("pageerror", (error) => problems.push(error.message));
    await page.emulateMedia({ colorScheme: scheme });
    await page.setViewportSize({ width: 820, height: 640 });
    const seen = await fakeOtpkeep(page);

    await page.goto("/gate.html#k=one-time");

    await expect(page.getByTestId("card")).toHaveCount(3);
    expect(new URL(page.url()).hash).toBe("");
    await expect(page.getByTestId("code").first()).toHaveText("111 456");
    // A note does not make a row taller.
    const heights = await page.getByTestId("card").evaluateAll((cards) => cards.map((c) => c.getBoundingClientRect().height));
    expect(new Set(heights).size).toBe(1);
    if (SHOTS) await page.screenshot({ path: `${SHOTS}/gate-${scheme}.png` });

    // By position: while editing, the title is an input, so the card has no matching text.
    const cloud = page.getByTestId("card").nth(2);
    await cloud.getByText("编辑").click();
    await cloud.getByLabel("备注").fill("运维用，备用码在 1Password");
    await cloud.getByText("保存").click();
    await expect(cloud.getByText("运维用，备用码在 1Password")).toBeVisible();

    await cloud.getByText("设置").click();
    await page.getByTestId("recorder").press("Control+Alt+Meta+KeyP");
    await expect(page.getByRole("status")).toHaveText("快捷键已设置，按下即可复制验证码");
    await expect(cloud.locator("kbd")).toHaveText("⌃⌥⌘P");
    if (SHOTS) await page.screenshot({ path: `${SHOTS}/gate-${scheme}-edited.png` });

    await page.getByTestId("card").first().getByText("复制").click();
    await expect(page.getByRole("status")).toHaveText("已复制 · 30 秒后从剪贴板清除");
    expect(seen.copied).toEqual(["e1"]);

    expect([...seen.bearers]).toEqual(["Bearer s1"]);
    expect(problems).toEqual([]);
  });
}

test("a page without a link explains how to open it", async ({ page }) => {
  await page.goto("/gate.html");

  await expect(page.getByText("会话已结束")).toBeVisible();
});
