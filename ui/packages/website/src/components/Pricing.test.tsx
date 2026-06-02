import { fireEvent, render, screen } from "@testing-library/react";
import { BrowserRouter } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { SUPPORT_EMAIL } from "../lib/contact";
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
    expect(banner).toHaveTextContent(RATES_DISPLAY.FREE_TRIAL_BANNER);
    expect(banner).toHaveTextContent(/Free until July 31, 2026/);
  });

  it("renders a simple three-row rate table (event, runtime, model tokens)", () => {
    renderPricing();
    const table = screen.getByTestId("pricing-rate-table");
    expect(table.tagName).toBe("DL");
    expect(table).toHaveTextContent(/Event receipt/i);
    expect(table).toHaveTextContent(/Active runtime/i);
    expect(table).toHaveTextContent(/Model tokens/i);
  });

  it("frames runtime as usage-based per-second, same rate both postures, no struck-through rates", () => {
    const { container } = renderPricing();
    const table = screen.getByTestId("pricing-rate-table");
    expect(table).toHaveTextContent(/active runtime/i);
    expect(table).toHaveTextContent(/only while a zombie is running/i);
    expect(table).toHaveTextContent(/platform or your own key/i);
    // No struck-through dual-rate presentation.
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
    const card = screen.getByTestId("pricing-rate-card");
    expect(card.textContent).toMatch(/billed by the second/i);
    expect(card.textContent).toMatch(/only while a zombie is actively working/i);
    expect(card.textContent).toMatch(/idle time/i);
  });

  it("renders the design-partner contact note", () => {
    renderPricing();
    const note = screen.getByTestId("pricing-design-partner-note");
    expect(note).toHaveTextContent(/design partner/i);
    expect(note.querySelector("a")).toHaveAttribute(
      "href",
      expect.stringContaining(SUPPORT_EMAIL),
    );
  });

  it("renders a single early-access CTA pointing at APP_BASE_URL", () => {
    renderPricing();
    const cta = screen.getByTestId("pricing-install-cta");
    expect(cta).toHaveAttribute("href", "https://app.dev.usezombie.com");
    expect(cta.textContent).toMatch(/get early access/i);
    expect(screen.queryByRole("link", { name: /upgrade/i })).not.toBeInTheDocument();
  });

  it("early-access CTA hugs its content (self-start) instead of stretching to card width", () => {
    renderPricing();
    expect(screen.getByTestId("pricing-install-cta").className).toMatch(/\bself-start\b/);
  });

  it("early-access CTA fires trackSignupStarted (NOT signupCompleted) with pricing_install source", () => {
    renderPricing();
    fireEvent.click(screen.getByTestId("pricing-install-cta"));
    expect(analytics.trackSignupStarted).toHaveBeenCalledWith({
      source: "pricing_install",
      surface: "pricing",
      mode: "humans",
    });
  });

  it("does not render the old Hobby/Scale tier ladder", () => {
    renderPricing();
    expect(screen.queryByRole("heading", { level: 2, name: /^Hobby$/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("heading", { level: 2, name: /^Scale$/ })).not.toBeInTheDocument();
    expect(screen.queryByTestId("pricing-card-hobby")).not.toBeInTheDocument();
    expect(screen.queryByTestId("pricing-card-scale")).not.toBeInTheDocument();
  });
});
