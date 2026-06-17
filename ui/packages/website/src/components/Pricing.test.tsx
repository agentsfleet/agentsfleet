import { fireEvent, render, screen } from "@testing-library/react";
import { BrowserRouter } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { SUPPORT_EMAIL } from "../lib/contact";
import { PRICING_COPY, PRICING_PLANS } from "../lib/marketing-copy";
import { RATES_DISPLAY } from "../lib/rates";

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

  it("leads with the free-trial banner from RATES_DISPLAY", () => {
    renderPricing();
    const banner = screen.getByTestId("pricing-free-trial-banner");
    expect(banner).toHaveTextContent(RATES_DISPLAY.FREE_TRIAL_PILL);
    expect(banner).toHaveTextContent(/Free until July 31, 2026/);
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

  it("renders usage early-access CTA disabled", () => {
    renderPricing();
    const cta = screen.getByTestId("pricing-cta-usage");
    expect(cta.tagName).toBe("BUTTON");
    expect(cta).toBeDisabled();
    expect(cta).not.toHaveAttribute("href");
    expect(cta.textContent).toMatch(/get early access/i);
    expect(screen.queryByRole("link", { name: /upgrade/i })).not.toBeInTheDocument();
  });

  it("pricing CTAs stretch inside their plan cards", () => {
    renderPricing();
    expect(screen.getByTestId("pricing-cta-usage").className).toMatch(/\bw-full\b/);
  });

  it("disabled early-access CTA does not track signup", () => {
    renderPricing();
    fireEvent.click(screen.getByTestId("pricing-cta-usage"));
    expect(analytics.trackSignupStarted).not.toHaveBeenCalled();
  });

  it("does not render the old Hobby/Scale tier ladder", () => {
    renderPricing();
    expect(screen.queryByRole("heading", { level: 2, name: /^Hobby$/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("heading", { level: 2, name: /^Scale$/ })).not.toBeInTheDocument();
    expect(screen.queryByTestId("pricing-card-hobby")).not.toBeInTheDocument();
    expect(screen.queryByTestId("pricing-card-scale")).not.toBeInTheDocument();
  });
});
