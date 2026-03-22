import { render, screen } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import { AnimatedIcon } from "@usezombie/design-system";

describe("AnimatedIcon", () => {
  it("renders children", () => {
    render(<AnimatedIcon>🖐️</AnimatedIcon>);
    expect(screen.getByText("🖐️")).toBeInTheDocument();
  });

  it("is aria-hidden when no label is provided (decorative)", () => {
    const { container } = render(<AnimatedIcon>👋</AnimatedIcon>);
    const wrapper = container.querySelector(".z-animated-icon");
    expect(wrapper).toHaveAttribute("aria-hidden", "true");
  });

  it("has role=img and aria-label when label is provided", () => {
    const { container } = render(<AnimatedIcon label="Zombie hand">🖐️</AnimatedIcon>);
    const el = container.querySelector(".z-animated-icon");
    expect(el).toBeInTheDocument();
    expect(el).toHaveAttribute("role", "img");
    expect(el).toHaveAttribute("aria-label", "Zombie hand");
    expect(el).not.toHaveAttribute("aria-hidden");
  });

  it("defaults to wave animation and self-hover trigger", () => {
    const { container } = render(<AnimatedIcon>🖐️</AnimatedIcon>);
    const wrapper = container.querySelector(".z-animated-icon");
    expect(wrapper).toHaveAttribute("data-animation", "wave");
    expect(wrapper).toHaveAttribute("data-trigger", "self-hover");
  });

  it("accepts animation=wiggle", () => {
    const { container } = render(<AnimatedIcon animation="wiggle">🖐️</AnimatedIcon>);
    const wrapper = container.querySelector(".z-animated-icon");
    expect(wrapper).toHaveAttribute("data-animation", "wiggle");
  });

  it("accepts trigger=parent-hover", () => {
    const { container } = render(<AnimatedIcon trigger="parent-hover">🖐️</AnimatedIcon>);
    const wrapper = container.querySelector(".z-animated-icon");
    expect(wrapper).toHaveAttribute("data-trigger", "parent-hover");
  });

  it("accepts trigger=always", () => {
    const { container } = render(<AnimatedIcon trigger="always">🖐️</AnimatedIcon>);
    const wrapper = container.querySelector(".z-animated-icon");
    expect(wrapper).toHaveAttribute("data-trigger", "always");
  });

  it("wraps children in a glyph span for animation", () => {
    const { container } = render(<AnimatedIcon>✋</AnimatedIcon>);
    const glyph = container.querySelector(".z-animated-icon__glyph");
    expect(glyph).toBeInTheDocument();
    expect(glyph?.textContent).toBe("✋");
  });

  it("merges custom className", () => {
    const { container } = render(<AnimatedIcon className="extra">🖐️</AnimatedIcon>);
    const wrapper = container.querySelector(".z-animated-icon");
    expect(wrapper).toHaveClass("z-animated-icon", "extra");
  });
});
