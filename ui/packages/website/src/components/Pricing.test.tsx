import { fireEvent, render, screen, within } from "@testing-library/react";
import { BrowserRouter } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { SUPPORT_EMAIL } from "../lib/contact";
import { PRICING_COPY, PRICING_PLANS } from "../lib/marketing-copy";
import { RATES_DISPLAY } from "../lib/rates";
import { WAITLIST_URL } from "../config";

const analytics = vi.hoisted(() => ({
  trackSignupStarted: vi.fn(),
}));

vi.mock("../analytics/posthog", async () => {
  const actual = await vi.importActual<typeof import("../analytics/posthog")>(
    "../analytics/posthog",
  );
  return {
    ...actual,
    trackSignupStarted: analytics.trackSignupStarted,
  };
});

import Pricing from "./Pricing";

function renderPricing() {
  return render(
    <BrowserRouter>
      <Pricing />
    </BrowserRouter>,
  );
}

describe("Pricing component", () => {
  beforeEach(() => {
    analytics.trackSignupStarted.mockReset();
  });

  it("leads with the early-access banner from RATES_DISPLAY", () => {
    renderPricing();
    const banner = screen.getByTestId("pricing-early-access-banner");
    expect(banner).toHaveTextContent(RATES_DISPLAY.EARLY_ACCESS_PILL);
    expect(banner).toHaveTextContent(/Free during early access/);
  });

  it("renders the three approved pricing cards", () => {
    renderPricing();
    expect(screen.getByText(PRICING_COPY.headline)).toBeInTheDocument();
    for (const plan of PRICING_PLANS) {
      const card = screen.getByTestId(`pricing-card-${plan.id}`);
      expect(card).toHaveTextContent(plan.name);
      for (const feature of plan.features) {
        expect(card).toHaveTextContent(feature);
      }
    }
  });

  it("frames runtime as usage-based per-second with no struck-through rates", () => {
    const { container } = renderPricing();
    const usage = screen.getByTestId("pricing-card-usage");
    expect(usage).toHaveTextContent(/metered only while running/i);
    expect(usage).toHaveTextContent(/pay as you go/i);
    expect(container.querySelector("s")).toBeNull();
  });

  it("renders rate values straight from the RATES_DISPLAY constants (display-only, no hardcoding)", () => {
    renderPricing();
    expect(screen.getByTestId("pricing-rate-event")).toHaveTextContent(
      RATES_DISPLAY.EVENT_RATE,
    );
    expect(screen.getByTestId("pricing-rate-run")).toHaveTextContent(
      RATES_DISPLAY.RUN_RATE_PER_SEC,
    );
    expect(screen.getByTestId("pricing-rate-run-hourly")).toHaveTextContent(
      RATES_DISPLAY.RUN_RATE_PER_HOUR,
    );
  });

  it("does not render the per-stage billing-flow grid (it buried the headline)", () => {
    renderPricing();
    expect(screen.queryByTestId("pricing-flow")).not.toBeInTheDocument();
    expect(screen.queryByTestId("pricing-flow-billed")).not.toBeInTheDocument();
    expect(screen.queryByTestId("pricing-flow-llm")).not.toBeInTheDocument();
    expect(screen.queryByTestId("pricing-stage-rates")).not.toBeInTheDocument();
  });

  it("does not render the operational-extras section", () => {
    renderPricing();
    expect(screen.queryByTestId("pricing-extras")).not.toBeInTheDocument();
    expect(screen.queryByText(/operational extras/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/provisioned per workspace/i)).not.toBeInTheDocument();
  });

  it("explains the usage-based per-second billing in plain language", () => {
    renderPricing();
    const card = screen.getByTestId("pricing-card-usage");
    expect(screen.getByText(PRICING_COPY.lede)).toHaveTextContent(/metered per second/i);
    expect(card.textContent).toMatch(/metered only while running/i);
  });

  it("renders the enterprise contact CTA", () => {
    renderPricing();
    expect(screen.getByTestId("pricing-cta-enterprise")).toHaveAttribute(
      "href",
      expect.stringContaining(SUPPORT_EMAIL),
    );
  });

  it("enabled enterprise contact CTA still tracks signup intent", () => {
    renderPricing();
    fireEvent.click(screen.getByTestId("pricing-cta-enterprise"));
    expect(analytics.trackSignupStarted).toHaveBeenCalledWith({
      source: "pricing_enterprise",
      surface: "pricing",
      mode: "humans",
    });
  });

  it("renders usage early-access CTA as a waitlist link", () => {
    renderPricing();
    const cta = screen.getByTestId("pricing-cta-usage");
    expect(cta.tagName).toBe("A");
    expect(cta).not.toBeDisabled();
    expect(cta).toHaveAttribute("href", WAITLIST_URL);
    expect(cta.textContent).toMatch(/get early access/i);
    // External (Clerk) host — opens in a new tab like every other external link.
    expect(cta).toHaveAttribute("target", "_blank");
    expect(cta).toHaveAttribute("rel", "noopener noreferrer");
    expect(screen.queryByRole("link", { name: /upgrade/i })).not.toBeInTheDocument();
  });

  it("routes the early-access Start-free call-to-action to the waitlist too", () => {
    renderPricing();
    const cta = screen.getByTestId("pricing-cta-early-access");
    expect(cta.tagName).toBe("A");
    expect(cta).toHaveAttribute("href", WAITLIST_URL);
    expect(cta.textContent).toMatch(/start free/i);
  });

  // The sibling sources were pinned; this one was not, which is how a plan-id
  // rename reached a PostHog property unnoticed. The card's DOM id is
  // kebab-case (`pricing-cta-early-access`) and its analytics source is
  // snake_case, so this asserts the bridge between the two conventions rather
  // than assuming they agree.
  it("reports the early-access signup source in PostHog's snake_case convention", () => {
    renderPricing();
    fireEvent.click(screen.getByTestId("pricing-cta-early-access"));
    expect(analytics.trackSignupStarted).toHaveBeenCalledWith({
      source: "pricing_early_access",
      surface: "pricing",
      mode: "humans",
    });
  });

  it("pricing CTAs stretch inside their plan cards", () => {
    renderPricing();
    expect(screen.getByTestId("pricing-cta-usage").className).toMatch(/\bw-full\b/);
  });

  it("usage early-access CTA tracks signup intent", () => {
    renderPricing();
    fireEvent.click(screen.getByTestId("pricing-cta-usage"));
    expect(analytics.trackSignupStarted).toHaveBeenCalledWith({
      source: "pricing_usage",
      surface: "pricing",
      mode: "humans",
    });
  });

  it("enterprise card surfaces the contact email as visible, selectable text and tracks it", () => {
    renderPricing();
    const note = screen.getByTestId("pricing-enterprise-email");
    expect(note).toHaveTextContent(SUPPORT_EMAIL);
    const emailLink = within(note).getByRole("link");
    expect(emailLink).toHaveAttribute("href", `mailto:${SUPPORT_EMAIL}`);
    // A lead who emails directly must still register in the funnel.
    fireEvent.click(emailLink);
    expect(analytics.trackSignupStarted).toHaveBeenCalledWith({
      source: "pricing_enterprise_email",
      surface: "pricing",
      mode: "humans",
    });
  });

  it("keeps the Enterprise mailto CTA in the same tab (no new-tab for a mailto)", () => {
    renderPricing();
    const cta = screen.getByTestId("pricing-cta-enterprise");
    expect(cta).not.toHaveAttribute("target");
  });

  it("does not render the old Hobby/Scale tier ladder", () => {
    renderPricing();
    expect(screen.queryByRole("heading", { level: 2, name: /^Hobby$/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("heading", { level: 2, name: /^Scale$/ })).not.toBeInTheDocument();
    expect(screen.queryByTestId("pricing-card-hobby")).not.toBeInTheDocument();
    expect(screen.queryByTestId("pricing-card-scale")).not.toBeInTheDocument();
  });
});
