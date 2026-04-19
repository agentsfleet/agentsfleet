import { render } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import ZombieHandIcon from "./ZombieHandIcon";

describe("ZombieHandIcon", () => {
  it("renders an SVG element", () => {
    const { container } = render(<ZombieHandIcon />);
    const svg = container.querySelector("svg");
    expect(svg).toBeInTheDocument();
    expect(svg).toHaveAttribute("viewBox", "0 0 64 64");
  });

  it("is aria-hidden by default (decorative)", () => {
    const { container } = render(<ZombieHandIcon />);
    const svg = container.querySelector("svg");
    expect(svg).toHaveAttribute("aria-hidden", "true");
  });

  it("defaults to size 20", () => {
    const { container } = render(<ZombieHandIcon />);
    const svg = container.querySelector("svg");
    expect(svg).toHaveAttribute("width", "20");
    expect(svg).toHaveAttribute("height", "20");
  });

  it("accepts a custom size", () => {
    const { container } = render(<ZombieHandIcon size={32} />);
    const svg = container.querySelector("svg");
    expect(svg).toHaveAttribute("width", "32");
    expect(svg).toHaveAttribute("height", "32");
  });

  it("passes through additional SVG props", () => {
    const { container } = render(<ZombieHandIcon className="custom-icon" data-testid="zombie" />);
    const svg = container.querySelector("svg");
    expect(svg).toHaveClass("custom-icon");
    expect(svg).toHaveAttribute("data-testid", "zombie");
  });

  it("contains hand path elements", () => {
    const { container } = render(<ZombieHandIcon />);
    const paths = container.querySelectorAll("path");
    expect(paths.length).toBeGreaterThanOrEqual(5);
  });
});
