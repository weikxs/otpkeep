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
