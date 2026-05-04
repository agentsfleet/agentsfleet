import { fireEvent, render, screen } from "@testing-library/react";
import { BrowserRouter } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";

const analytics = vi.hoisted(() => ({
  trackNavigationClicked: vi.fn(),
  trackSignupStarted: vi.fn(),
}));

vi.mock("../analytics/posthog", () => analytics);

import Hero from "./Hero";
import { DOCS_QUICKSTART_URL } from "../config";

function renderHero() {
  return render(
    <BrowserRouter>
      <Hero />
    </BrowserRouter>
  );
}

describe("Hero", () => {
  beforeEach(() => {
    analytics.trackNavigationClicked.mockReset();
    analytics.trackSignupStarted.mockReset();
  });

  it("renders h1 with both headline lines", () => {
    const { container } = renderHero();
    const h1 = container.querySelector("h1");
    expect(h1).toHaveTextContent(/operational outcomes/i);
    expect(h1).toHaveTextContent(/don't fall into limbo/i);
  });

  it("shows the human badge text", () => {
    renderHero();
    expect(screen.getByText(/durable agent runtime/i)).toBeInTheDocument();
  });

  it("renders the primary CTA linking to docs quickstart", () => {
    const { container } = renderHero();
    const cta = Array.from(container.querySelectorAll("a")).find((link) =>
      link.textContent?.match(/install platform-ops/i),
    );
    expect(cta).toHaveAttribute("href", DOCS_QUICKSTART_URL);
  });

  it("renders the pricing CTA", () => {
    const { container } = renderHero();
    const cta = Array.from(container.querySelectorAll("a")).find((link) => link.textContent?.match(/see pricing/i));
    expect(cta).toHaveAttribute("href", "/pricing");
  });

  it("tracks clicks on the primary CTA", () => {
    renderHero();
    fireEvent.click(screen.getByRole("link", { name: /install platform-ops/i }));
    expect(analytics.trackSignupStarted).toHaveBeenCalledWith({
      source: "hero_primary",
      surface: "hero",
      mode: "humans",
    });
  });

  it("tracks clicks on the pricing CTA", () => {
    renderHero();
    fireEvent.click(screen.getByRole("link", { name: /see pricing/i }));
    expect(analytics.trackNavigationClicked).toHaveBeenCalledWith({
      source: "hero_secondary_pricing",
      surface: "hero",
      target: "pricing",
    });
  });

  it("renders the terminal with quick start label", () => {
    renderHero();
    expect(screen.getByLabelText(/quick start command/i)).toBeInTheDocument();
  });

  it("renders copy button on terminal", () => {
    renderHero();
    expect(screen.getByTestId("copy-btn")).toBeInTheDocument();
  });

  it("renders the data-command attribute on the terminal", () => {
    renderHero();
    const terminal = screen.getByLabelText(/quick start command/i);
    expect(terminal).toHaveAttribute("data-command", "npm install -g @usezombie/zombiectl");
  });

  it("does not render the removed proof cards", () => {
    renderHero();
    expect(screen.queryByText("Validated PRs")).not.toBeInTheDocument();
    expect(screen.queryByText("Direct model billing")).not.toBeInTheDocument();
    expect(screen.queryByText("Measurable run quality")).not.toBeInTheDocument();
  });
});
