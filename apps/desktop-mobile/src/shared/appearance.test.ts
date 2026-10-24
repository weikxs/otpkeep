import { describe, expect, it } from "vitest";
import { applyTheme, languageOf } from "./appearance";

describe("appearance", () => {
  it("marks a chosen theme and clears it for the system", () => {
    const root = document.createElement("html");

    applyTheme("dark", root);
    expect(root.dataset.theme).toBe("dark");
    applyTheme("system", root);
    expect(root.dataset.theme).toBeUndefined();
  });

  it("uses the chosen language or the system's", () => {
    expect(languageOf("en", "zh-CN")).toBe("en");
    expect(languageOf("system", "zh-CN")).toBe("zh-CN");
  });
});
