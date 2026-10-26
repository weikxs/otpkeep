// @vitest-environment node
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

interface TauriConfig {
  app: { security: { csp: string; freezePrototype: boolean }; macOSPrivateApi?: boolean };
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

describe("macOS private API", () => {
  it("stays off: no window is transparent since the menu is drawn by the system", () => {
    expect(config.app.macOSPrivateApi ?? false).toBe(false);
  });
});

describe("desktop window settings", () => {
  const macos = windowsFor("macos");

  it("starts in the menu bar: only the search panel is configured, hidden, and the app makes it when asked", () => {
    expect(macos.map((w) => w.label)).toEqual(["search"]);
    expect(macos.every((w) => w.visible === false)).toBe(true);
    expect(macos.every((w) => w.create === false)).toBe(true);
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
