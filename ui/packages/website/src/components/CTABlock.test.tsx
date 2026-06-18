import { render, screen } from "@testing-library/react";
import { BrowserRouter } from "react-router-dom";
import { describe, it, expect } from "vitest";
import CTABlock from "./CTABlock";
import { CTA_COPY } from "../lib/marketing-copy";

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
      screen.getByRole("heading", { level: 2, name: CTA_COPY.heading }),
    ).toBeInTheDocument();
  });

  it("speaks to the operator, not the machine API", () => {
    renderCtaBlock();
    expect(screen.getByText(CTA_COPY.lede)).toBeInTheDocument();
    // The OpenAPI / machine-surface pitch belongs on /agents, not the human closer.
    expect(screen.queryByText(/openapi 3\.1/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/machine surface/i)).not.toBeInTheDocument();
  });

  it("renders quickstart CTA with correct href", () => {
    renderCtaBlock();
    const cta = screen.getByRole("link", { name: /read quickstart/i });
    expect(cta).toHaveAttribute("href", "https://docs.agentsfleet.net/quickstart");
  });

  it("renders pricing CTA as anchor to home #pricing section", () => {
    renderCtaBlock();
    const cta = screen.getByRole("link", { name: /view pricing/i });
    expect(cta).toHaveAttribute("href", "/#pricing");
  });

  it("places the heading on the page left rail, not inside the reading-measure column", () => {
    const { container } = renderCtaBlock();
    const heading = screen.getByRole("heading", {
      level: 2,
      name: CTA_COPY.heading,
    });
    // Heading aligns with the page's left rail; the prose + buttons keep the
    // narrower reading measure.
    expect(heading.closest(".max-w-measure")).toBeNull();
    const measure = container.querySelector(".max-w-measure");
    expect(measure).not.toBeNull();
    expect(measure!.textContent).toMatch(/keeps recurring/i);
  });
});
