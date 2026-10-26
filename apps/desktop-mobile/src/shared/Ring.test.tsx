import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { render } from "@testing-library/preact";
import { describe, expect, it } from "vitest";
import { Ring } from "./Ring";

// A code that ends at a whole period, seen 10 seconds into its 30.
const ENDS = 1_700_000_030_000;
const NOW = ENDS - 20_000;

function ring(changes: Partial<Parameters<typeof Ring>[0]> = {}) {
  return (
    <Ring remaining={20} period={30} ending={false} label="20" validUntilMs={ENDS} coreNow={NOW} epoch={1} {...changes} />
  );
}

const value = (container: Element) => container.querySelector(".ring-value") as SVGCircleElement;

describe("Ring", () => {
  it("empties itself with one animation that starts where the period stands", () => {
    const { container } = render(ring());

    const circle = value(container);
    expect(circle.style.getPropertyValue("--ring-period")).toBe("30s");
    expect(circle.style.getPropertyValue("--ring-delay")).toBe("-10000ms");
    expect(circle.getAttribute("stroke-dasharray")).toBeNull();
  });

  it("keeps the same animation while the seconds count down", () => {
    const { container, rerender } = render(ring());
    const before = value(container);

    rerender(ring({ remaining: 19, label: "19", coreNow: NOW + 1_000 }));

    expect(value(container)).toBe(before);
    expect(value(container).style.getPropertyValue("--ring-delay")).toBe("-10000ms");
  });

  it("starts again with a new code", () => {
    const { container, rerender } = render(ring());
    const before = value(container);

    rerender(ring({ remaining: 30, label: "30", validUntilMs: ENDS + 30_000, coreNow: ENDS }));

    expect(value(container)).not.toBe(before);
    expect(value(container).style.getPropertyValue("--ring-delay")).toBe("0ms");
  });

  it("starts again after the window reloaded its codes", () => {
    const { container, rerender } = render(ring());
    const before = value(container);

    rerender(ring({ remaining: 15, label: "15", coreNow: NOW + 5_000, epoch: 2 }));

    expect(value(container)).not.toBe(before);
    expect(value(container).style.getPropertyValue("--ring-delay")).toBe("-15000ms");
  });
});

describe("ring.css", () => {
  const css = readFileSync(resolve(process.cwd(), "src/shared/ring.css"), "utf8");

  it("has no transitions, which kept the window drawing after every tick", () => {
    expect(css).not.toMatch(/transition/u);
  });

  it("animates the ring and steps it once a second for reduced motion", () => {
    expect(css).toMatch(/@keyframes ring-drain/u);
    expect(css).toMatch(/prefers-reduced-motion: reduce[^}]*\{[^}]*steps\(/u);
  });
});
