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

  it("renders the two-line mono headline", () => {
    const { container } = renderHero();
    const h1 = container.querySelector("h1");
    expect(h1).not.toBeNull();
    expect(h1).toHaveTextContent(/your deploy failed/i);
    expect(h1).toHaveTextContent(/the daemon already knows why/i);
    expect(h1!.className).toContain("font-mono");
  });

  it("renders the LIVE eyebrow with a WakePulse data-live=true mark", () => {
    renderHero();
    const eyebrow = screen.getByTestId("hero-eyebrow");
    expect(eyebrow.textContent).toMatch(/LIVE — wake\.on\.event/i);
    const pulse = eyebrow.querySelector("[data-live=\"true\"]");
    expect(pulse).not.toBeNull();
  });

  it("renders the lede paragraph in the spec voice", () => {
    renderHero();
    expect(screen.getByText(/long-lived runtime that owns one operational outcome/i)).toBeInTheDocument();
    expect(screen.getByText(/durable, replayable log/i)).toBeInTheDocument();
  });

  it("renders the primary CTA pointing at the docs quickstart", () => {
    renderHero();
    const cta = screen.getByTestId("hero-cta-primary");
    expect(cta).toHaveAttribute("href", DOCS_QUICKSTART_URL);
    expect(cta.textContent).toMatch(/install in claude code/i);
  });

  it("renders the secondary CTA pointing at /agents", () => {
    renderHero();
    const cta = screen.getByTestId("hero-cta-secondary");
    expect(cta).toHaveAttribute("href", "/agents");
    expect(cta.textContent).toMatch(/view a real wake/i);
  });

  it("tracks clicks on the primary CTA", () => {
    renderHero();
    fireEvent.click(screen.getByTestId("hero-cta-primary"));
    expect(analytics.trackSignupStarted).toHaveBeenCalledWith({
      source: "hero_primary",
      surface: "hero",
      mode: "humans",
    });
  });

  it("tracks clicks on the secondary CTA", () => {
    renderHero();
    fireEvent.click(screen.getByTestId("hero-cta-secondary"));
    expect(analytics.trackNavigationClicked).toHaveBeenCalledWith({
      source: "hero_secondary_replay",
      surface: "hero",
      target: "agents",
    });
  });

  it("renders the install transcript Terminal", () => {
    renderHero();
    expect(screen.getByTestId("hero-cli")).toBeInTheDocument();
    expect(screen.getByLabelText(/install platform-ops via claude code/i)).toBeInTheDocument();
  });

  it("does not render orange-era hero scaffolding", () => {
    const { container } = renderHero();
    expect(container.querySelector(".hero-illustration")).toBeNull();
    expect(container.querySelector(".hero-proof-grid")).toBeNull();
    expect(container.querySelector(".hero-cta-primary")).toBeNull();
    expect(container.querySelector(".hero-headline")).toBeNull();
  });
});
