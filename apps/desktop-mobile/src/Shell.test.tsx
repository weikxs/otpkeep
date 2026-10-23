import { render, screen } from "@testing-library/preact";
import { describe, expect, it } from "vitest";
import { Shell } from "./Shell";

describe("Shell", () => {
  it("names the product", () => {
    render(<Shell surface="app" />);

    expect(screen.getByRole("heading", { level: 1 }).textContent).toBe("OTPKeep");
  });

  it("says which surface it runs on", () => {
    render(<Shell surface="gate" />);

    expect(screen.getByTestId("surface").textContent).toBe("Browser management page");
  });
});
