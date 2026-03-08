import { render, screen } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import CTABlock from "./CTABlock";

describe("CTABlock", () => {
  it("renders the heading", () => {
    render(<CTABlock />);
    expect(screen.getByRole("heading", { level: 2, name: /queue work\. review prs\. sleep\./i })).toBeInTheDocument();
  });

  it("renders the description", () => {
    render(<CTABlock />);
    expect(screen.getByText(/start with Hobby, then move to Team/i)).toBeInTheDocument();
  });

  it("renders Start free CTA with correct href", () => {
    render(<CTABlock />);
    const cta = screen.getByRole("link", { name: /start free/i });
    expect(cta).toHaveAttribute("href", "https://docs.usezombie.com/quickstart");
  });

  it("renders Book team pilot CTA with mailto", () => {
    render(<CTABlock />);
    const cta = screen.getByRole("link", { name: /book team pilot/i });
    expect(cta).toHaveAttribute("href", expect.stringContaining("mailto:team@usezombie.com"));
  });
});
