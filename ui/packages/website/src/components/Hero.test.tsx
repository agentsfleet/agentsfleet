import { fireEvent, render, screen } from "@testing-library/react";
import { BrowserRouter } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";

const analytics = vi.hoisted(() => ({
  trackNavigationClicked: vi.fn(),
  trackSignupStarted: vi.fn(),
}));

vi.mock("../analytics/posthog", () => analytics);

import Hero from "./Hero";
import {
  HERO_PRIMARY_LABEL,
  HERO_SECONDARY_LABEL,
  HOW_IT_WORKS_ANCHOR_ID,
  PILLAR_TOKENS,
} from "../lib/marketing-copy";
import { WAITLIST_URL } from "../config";

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

  // The hero once opened with a kicker carrying a mint dot (FINDING-M05). The
  // dot never meant anything was live, and the kicker only restated the
  // headline; both are gone. The guarantee is now structural: the hero shows no
  // live indicator at all, because it has no element that could imply one.
  it("does not imply live activity in the illustrative hero", () => {
    const { container } = renderHero();
    expect(screen.queryByTestId("hero-eyebrow")).not.toBeInTheDocument();
    expect(container.querySelector("[data-live=\"true\"]")).toBeNull();
  });

  it("renders every pillar token in the hero", () => {
    renderHero();
    const hero = screen.getByTestId("hero").textContent;
    for (const token of PILLAR_TOKENS) {
      expect(hero, `hero missing pillar token: ${token}`).toContain(token);
    }
  });

  it("emphasises the trigger and the sources, both pillar tokens", () => {
    const { container } = renderHero();
    const emphasised = [...container.querySelectorAll("strong")].map((el) => el.textContent);
    expect(emphasised).toEqual([PILLAR_TOKENS[0], PILLAR_TOKENS[1]]);
  });

  it("renders early-access as a waitlist link and loop CTA", () => {
    renderHero();
    const earlyAccess = screen.getByTestId("hero-cta-early-access");
    expect(earlyAccess).toHaveTextContent(HERO_PRIMARY_LABEL);
    // Now an enabled anchor to the Clerk-hosted waitlist, not a disabled button.
    expect(earlyAccess.tagName).toBe("A");
    expect(earlyAccess).not.toBeDisabled();
    expect(earlyAccess).toHaveAttribute("href", WAITLIST_URL);
    expect(earlyAccess).toHaveAttribute("target", "_blank");
    expect(earlyAccess).toHaveAttribute("rel", "noopener noreferrer");
    expect(screen.getByTestId("hero-cta-secondary")).toHaveTextContent(HERO_SECONDARY_LABEL);
    expect(screen.getByTestId("hero-cta-secondary")).toHaveAttribute(
      "href",
      `/#${HOW_IT_WORKS_ANCHOR_ID}`,
    );
    expect(screen.queryByTestId("hero-install-command")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /copy the install command/i })).not.toBeInTheDocument();
  });

  it("tracks a signup when the early-access waitlist CTA is clicked", () => {
    renderHero();
    fireEvent.click(screen.getByTestId("hero-cta-early-access"));
    expect(analytics.trackSignupStarted).toHaveBeenCalledWith({
      source: "hero_early_access",
      surface: "hero",
      mode: "humans",
    });
  });

  it("invites early access without promising a price or credit", () => {
    renderHero();
    const pill = screen.getByTestId("hero-promo-pill");
    expect(pill.tagName).toBe("A");
    expect(pill).toHaveAttribute("href", "/#pricing");
    expect(pill.textContent).toMatch(/help shape agentsfleet/i);
    expect(pill.textContent).not.toMatch(/\$\d|starter credit|free/i);
  });

  it("places the promo pill before the headline in document order", () => {
    renderHero();
    const pill = screen.getByTestId("hero-promo-pill");
    const headline = screen.getByTestId("hero-headline");
    expect(
      pill.compareDocumentPosition(headline) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });

  it("tracks clicks on the promo pill", () => {
    renderHero();
    fireEvent.click(screen.getByTestId("hero-promo-pill"));
    expect(analytics.trackNavigationClicked).toHaveBeenCalledWith({
      source: "hero_promo_pill",
      surface: "hero",
      target: "pricing",
    });
  });
});
