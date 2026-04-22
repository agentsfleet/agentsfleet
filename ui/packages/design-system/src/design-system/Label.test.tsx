import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { Label } from "./Label";

describe("Label", () => {
  it("renders a <label> with the provided text", () => {
    render(<Label htmlFor="email">Email address</Label>);
    const el = screen.getByText("Email address");
    expect(el.tagName).toBe("LABEL");
    expect(el).toHaveAttribute("for", "email");
  });

  it("applies semantic utilities", () => {
    render(<Label data-testid="l">Name</Label>);
    const cls = screen.getByTestId("l").className;
    expect(cls).toContain("text-sm");
    expect(cls).toContain("font-medium");
  });

  it("merges a custom className", () => {
    render(<Label className="extra" data-testid="l">Name</Label>);
    expect(screen.getByTestId("l").className).toContain("extra");
  });
});
