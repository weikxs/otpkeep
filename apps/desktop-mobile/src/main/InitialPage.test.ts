import { describe, expect, it } from "vitest";
import { takeInitialPage } from "./api";

describe("the page a new main window opens at", () => {
  it("is read once from the address and then removed", () => {
    location.hash = "#page=edit%3Aentry-1";

    expect(takeInitialPage()).toBe("edit:entry-1");
    expect(location.hash).toBe("");
    expect(takeInitialPage()).toBeNull();
  });
});
