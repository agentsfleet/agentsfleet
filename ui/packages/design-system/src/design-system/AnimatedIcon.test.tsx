import { describe, it, expect } from "vitest";
import { render } from "@testing-library/react";
import AnimatedIcon from "./AnimatedIcon";

describe("AnimatedIcon", () => {
  it("renders children", () => {
    const { container } = render(<AnimatedIcon>🖐️</AnimatedIcon>);
    expect(container.textContent).toContain("🖐️");
  });

  it("wraps children in the glyph element with data-animated-glyph", () => {
    const { container } = render(<AnimatedIcon>👋</AnimatedIcon>);
    const glyph = container.querySelector("[data-animated-glyph]");
    expect(glyph).toBeInTheDocument();
    expect(glyph?.className).toContain("origin-[70%_70%]");
  });

  it("is decorative (aria-hidden) when no label provided", () => {
    const { container } = render(<AnimatedIcon>🖐️</AnimatedIcon>);
    const wrapper = container.firstChild as HTMLElement;
    expect(wrapper.getAttribute("aria-hidden")).toBe("true");
    expect(wrapper.hasAttribute("role")).toBe(false);
  });

  it("becomes role=img with aria-label when label is provided", () => {
    const { container } = render(<AnimatedIcon label="Zombie hand">🖐️</AnimatedIcon>);
    const wrapper = container.firstChild as HTMLElement;
    expect(wrapper.getAttribute("role")).toBe("img");
    expect(wrapper.getAttribute("aria-label")).toBe("Zombie hand");
    expect(wrapper.hasAttribute("aria-hidden")).toBe(false);
  });

  it("self-hover trigger uses hover:/focus-visible: variants (default)", () => {
    const { container } = render(<AnimatedIcon animation="wave">x</AnimatedIcon>);
    const glyph = container.querySelector("[data-animated-glyph]") as HTMLElement;
    expect(glyph.className).toContain("hover:animate-wave");
    expect(glyph.className).toContain("focus-visible:animate-wave");
  });

  it("always trigger applies animate-* unconditionally", () => {
    const { container } = render(
      <AnimatedIcon trigger="always" animation="wiggle">
        x
      </AnimatedIcon>,
    );
    const glyph = container.querySelector("[data-animated-glyph]") as HTMLElement;
    expect(glyph.className).toContain("animate-wiggle");
    expect(glyph.className).not.toContain("hover:animate-wiggle");
  });

  it("parent-hover trigger uses group-hover: variants (composes with ancestor `.group`)", () => {
    const { container } = render(
      <AnimatedIcon trigger="parent-hover" animation="wiggle">
        x
      </AnimatedIcon>,
    );
    const glyph = container.querySelector("[data-animated-glyph]") as HTMLElement;
    expect(glyph.className).toContain("group-hover:animate-wiggle");
    expect(glyph.className).toContain("group-focus-visible:animate-wiggle");
  });

  it("merges extra className on the outer wrapper", () => {
    const { container } = render(<AnimatedIcon className="extra">x</AnimatedIcon>);
    expect((container.firstChild as HTMLElement).className).toContain("extra");
  });

  it("drop animation maps to animate-drop with parent-hover trigger", () => {
    const { container } = render(
      <AnimatedIcon trigger="parent-hover" animation="drop">
        x
      </AnimatedIcon>,
    );
    const glyph = container.querySelector("[data-animated-glyph]") as HTMLElement;
    expect(glyph.className).toContain("group-hover:animate-drop");
    expect(glyph.className).toContain("group-focus-visible:animate-drop");
  });

  it("drop animation can fire unconditionally with trigger=always", () => {
    const { container } = render(
      <AnimatedIcon trigger="always" animation="drop">
        x
      </AnimatedIcon>,
    );
    const glyph = container.querySelector("[data-animated-glyph]") as HTMLElement;
    expect(glyph.className).toContain("animate-drop");
    expect(glyph.className).not.toContain("hover:animate-drop");
  });

  it("drop animation with self-hover trigger uses hover:/focus-visible: variants", () => {
    const { container } = render(
      <AnimatedIcon trigger="self-hover" animation="drop">
        x
      </AnimatedIcon>,
    );
    const glyph = container.querySelector("[data-animated-glyph]") as HTMLElement;
    expect(glyph.className).toContain("hover:animate-drop");
    expect(glyph.className).toContain("focus-visible:animate-drop");
    // self-hover must not also emit the parent-hover utilities — those are
    // mutually exclusive and would double-fire if both were present.
    expect(glyph.className).not.toContain("group-hover:animate-drop");
  });

  it("[data-animated-glyph] is present so prefers-reduced-motion CSS can disable any animation", () => {
    // The reduced-motion guard in tokens.css matches `[data-animated-glyph]`
    // and resets `animation: none`. Pin the attribute hook on every animation
    // — losing it silently strips the a11y safety net.
    for (const animation of ["wave", "wiggle", "drop", "drop-overflow"] as const) {
      const { container, unmount } = render(
        <AnimatedIcon trigger="always" animation={animation}>
          x
        </AnimatedIcon>,
      );
      const glyph = container.querySelector("[data-animated-glyph]");
      expect(glyph, `missing data-animated-glyph for animation=${animation}`).not.toBeNull();
      unmount();
    }
  });

  it("drop-overflow animation maps to animate-drop-overflow with parent-hover trigger", () => {
    const { container } = render(
      <AnimatedIcon trigger="parent-hover" animation="drop-overflow">
        x
      </AnimatedIcon>,
    );
    const glyph = container.querySelector("[data-animated-glyph]") as HTMLElement;
    expect(glyph.className).toContain("group-hover:animate-drop-overflow");
    expect(glyph.className).toContain("group-focus-visible:animate-drop-overflow");
    // drop-overflow must not collapse to drop — they are distinct keyframes.
    expect(glyph.className).not.toContain("group-hover:animate-drop ");
  });
});
