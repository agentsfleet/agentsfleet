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

  it("renders h1 with the single-line punchy claim", () => {
    const { container } = renderHero();
    const h1 = container.querySelector("h1");
    expect(h1).toHaveTextContent(/agents that wake on every event/i);
  });

  it("shows the human badge text", () => {
    renderHero();
    expect(screen.getByText(/always-on event-driven runtime/i)).toBeInTheDocument();
  });

  it("renders the $5 starter credit hook in the command-card note", () => {
    renderHero();
    // "$5 starter credit" now appears twice — in the secondary CTA and in the
    // note's <strong> emphasis. Both are intentional; assert both, then pin
    // the no-card-required tail to the note.
    const matches = screen.getAllByText(/\$5 starter credit/i);
    expect(matches.length).toBeGreaterThanOrEqual(2);
    expect(matches.some((m) => m.tagName === "STRONG")).toBe(true);
    expect(screen.getByText(/no card required/i)).toBeInTheDocument();
  });

  it("renders the primary CTA linking to docs quickstart", () => {
    const { container } = renderHero();
    const cta = Array.from(container.querySelectorAll("a")).find((link) =>
      link.textContent?.match(/start an agent/i),
    );
    expect(cta).toHaveAttribute("href", DOCS_QUICKSTART_URL);
  });

  it("renders the offer-led pricing CTA pointing at /pricing", () => {
    const { container } = renderHero();
    const cta = Array.from(container.querySelectorAll("a")).find((link) =>
      link.textContent?.match(/\$5 starter credit/i),
    );
    expect(cta).toHaveAttribute("href", "/pricing");
  });

  it("kicker carries the always-on event-driven story (webhook → Zombie Agent → event log)", () => {
    renderHero();
    expect(screen.getByText(/webhook wakes the zombie agent/i)).toBeInTheDocument();
    expect(screen.getByText(/replayable event log/i)).toBeInTheDocument();
  });

  it("tracks clicks on the primary CTA", () => {
    renderHero();
    fireEvent.click(screen.getByRole("link", { name: /start an agent/i }));
    expect(analytics.trackSignupStarted).toHaveBeenCalledWith({
      source: "hero_primary",
      surface: "hero",
      mode: "humans",
    });
  });

  it("tracks clicks on the pricing CTA", () => {
    renderHero();
    fireEvent.click(screen.getByRole("link", { name: /\$5 starter credit/i }));
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
