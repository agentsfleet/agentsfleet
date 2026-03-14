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
  it("renders the heading", () => {
    renderCtaBlock();
    expect(screen.getByRole("heading", { level: 2, name: /wire autonomous agents into the control plane\./i })).toBeInTheDocument();
  });

  it("renders the description", () => {
    renderCtaBlock();
    expect(screen.getByText(/without mixing agent traffic into the human launch funnel/i)).toBeInTheDocument();
  });

  it("renders quickstart CTA with correct href", () => {
    renderCtaBlock();
    const cta = screen.getByRole("link", { name: /read quickstart/i });
    expect(cta).toHaveAttribute("href", "https://docs.usezombie.com/quickstart");
  });

  it("renders pricing CTA as internal link", () => {
    renderCtaBlock();
    const cta = screen.getByRole("link", { name: /view pricing/i });
    expect(cta).toHaveAttribute("href", "/pricing");
  });
});
