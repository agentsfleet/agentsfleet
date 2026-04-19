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
});
