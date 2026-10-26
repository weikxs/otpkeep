// @vitest-environment node
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

interface TauriConfig {
  app: { security: { csp: string; freezePrototype: boolean } };
}

const config = JSON.parse(
  readFileSync(new URL("../src-tauri/tauri.conf.json", import.meta.url), "utf8"),
) as TauriConfig;

describe("tauri.conf.json security settings", () => {
  it("keeps freezePrototype off, because it breaks every IPC call on iOS (BUG-001)", () => {
    expect(config.app.security.freezePrototype).toBe(false);
  });

  it("keeps a strict content security policy", () => {
    const csp = config.app.security.csp;

    expect(csp).toContain("default-src 'self'");
    expect(csp).toContain("script-src 'self'");
    expect(csp).toContain("object-src 'none'");
    expect(csp).not.toContain("unsafe-inline");
    expect(csp).not.toContain("unsafe-eval");
    expect(csp).not.toMatch(/https?:\/\/(?!ipc\.localhost)/);
  });
});

interface WindowConfig {
  label: string;
  url?: string;
  visible?: boolean;
  create?: boolean;
  decorations?: boolean;
}

function windowsFor(os: string): WindowConfig[] {
  const raw = readFileSync(new URL(`../src-tauri/tauri.${os}.conf.json`, import.meta.url), "utf8");
  return (JSON.parse(raw) as { app: { windows: WindowConfig[] } }).app.windows;
}

describe("desktop window settings", () => {
  const macos = windowsFor("macos");

  it("starts in the menu bar: the panels made hidden, the main window only when asked for", () => {
    expect(macos.map((w) => w.label)).toEqual(["panel", "search"]);
    expect(macos.every((w) => w.visible === false)).toBe(true);
  });

  it("leaves making the panels to the app, which closes them while they are not used", () => {
    expect(macos.every((w) => w.create === false)).toBe(true);
  });

  it("shows the panel without a title bar", () => {
    const panel = macos.find((w) => w.label === "panel");

    expect(panel?.url).toBe("panel.html");
    expect(panel?.decorations).toBe(false);
  });

  it("shows the search panel without a title bar", () => {
    const search = macos.find((w) => w.label === "search");

    expect(search?.url).toBe("search.html");
    expect(search?.decorations).toBe(false);
  });

  it("uses the same windows on every desktop system", () => {
    expect(windowsFor("windows")).toEqual(macos);
    expect(windowsFor("linux")).toEqual(macos);
  });
});
