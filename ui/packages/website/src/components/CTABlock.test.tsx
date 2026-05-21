import { render, screen } from "@testing-library/react";
import { BrowserRouter } from "react-router-dom";
import { describe, it, expect } from "vitest";
import CTABlock from "./CTABlock";

function renderCtaBlock() {
  return render(
    <BrowserRouter>
      <CTABlock />
    </BrowserRouter>
  );
}

describe("CTABlock", () => {
  it("renders the human-voice heading", () => {
    renderCtaBlock();
    expect(
      screen.getByRole("heading", { level: 2, name: /stop chasing failed deploys/i }),
    ).toBeInTheDocument();
  });

  it("speaks to the operator, not the machine API", () => {
    renderCtaBlock();
    expect(screen.getByText(/install one zombie, wire one webhook/i)).toBeInTheDocument();
    // The OpenAPI / machine-surface pitch belongs on /agents, not the human closer.
    expect(screen.queryByText(/openapi 3\.1/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/machine surface/i)).not.toBeInTheDocument();
  });

  it("renders quickstart CTA with correct href", () => {
    renderCtaBlock();
    const cta = screen.getByRole("link", { name: /read quickstart/i });
    expect(cta).toHaveAttribute("href", "https://docs.usezombie.com/quickstart");
  });

  it("renders pricing CTA as anchor to home #pricing section", () => {
    renderCtaBlock();
    const cta = screen.getByRole("link", { name: /view pricing/i });
    expect(cta).toHaveAttribute("href", "/#pricing");
  });
});
