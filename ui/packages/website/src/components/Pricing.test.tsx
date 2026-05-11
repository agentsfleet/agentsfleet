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

  it("renders the rate line with EVENT_RATE per event", () => {
    renderPricing();
    expect(screen.getByTestId("pricing-rate-event")).toHaveTextContent(RATES_DISPLAY.EVENT_RATE);
    expect(screen.getByTestId("pricing-rate-line")).toHaveTextContent(/per event receipt/i);
  });

  it("renders both stage rates side-by-side with the 10× gradient framing", () => {
    renderPricing();
    expect(screen.getByTestId("pricing-rate-stage-platform")).toHaveTextContent(
      RATES_DISPLAY.STAGE_PLATFORM,
    );
    expect(screen.getByTestId("pricing-rate-stage-self-managed")).toHaveTextContent(
      RATES_DISPLAY.STAGE_SELF_MANAGED,
    );
    const rates = screen.getByTestId("pricing-stage-rates");
    expect(rates).toHaveTextContent(/platform default/i);
    expect(rates).toHaveTextContent(/self-managed/i);
    expect(rates).toHaveTextContent(/10× cheaper to scale/i);
  });

  it("renders the introductory-rate subscript so future ratchets are expected", () => {
    renderPricing();
    expect(screen.getByTestId("pricing-introductory-rate-note")).toHaveTextContent(
      /stealth-mode testing rate — will rise post-GA/i,
    );
  });

  it("renders the STARTER_CREDIT badge", () => {
    renderPricing();
    const badge = screen.getByText(/starter credit, never expires/i);
    expect(badge).toHaveTextContent(
      `${RATES_DISPLAY.STARTER_CREDIT} starter credit, never expires`,
    );
  });

  it("does not render the dropped worked-example math line", () => {
    renderPricing();
    expect(screen.queryByTestId("pricing-worked-example")).not.toBeInTheDocument();
  });

  it("explains what a stage is in plain language", () => {
    renderPricing();
    const card = screen.getByTestId("pricing-rate-card");
    expect(card.textContent).toMatch(/stage is one reasoning step/i);
    expect(card.textContent).toMatch(/most diagnoses resolve in 1.{0,3}5 stages/i);
  });

  it("does not surface the BYOK provider list paragraph in the rate card (the diagram below covers it)", () => {
    renderPricing();
    const card = screen.getByTestId("pricing-rate-card");
    expect(card.textContent).not.toMatch(
      /Self-managed on Anthropic, OpenAI, Fireworks, Together, Groq, Moonshot/i,
    );
  });

  it("renders the stealth-mode banner with design-partner contact", () => {
    renderPricing();
    const note = screen.getByTestId("pricing-design-partner-note");
    expect(note).toHaveTextContent(/stealth-mode testing/i);
    expect(note).toHaveTextContent(/design partner/i);
    expect(note.querySelector("a")).toHaveAttribute(
      "href",
      expect.stringContaining(SUPPORT_EMAIL),
    );
  });

  it("renders the billing flow diagram with one event + three stage cells using the new rates", () => {
    renderPricing();
    const flow = screen.getByTestId("pricing-flow");
    expect(flow).toBeInTheDocument();
    const billed = screen.getByTestId("pricing-flow-billed");
    const cells = billed.querySelectorAll('[data-testid^="pricing-flow-cell-"]');
    expect(cells).toHaveLength(4);
    expect(screen.getByTestId("pricing-flow-cell-event")).toHaveTextContent(
      RATES_DISPLAY.EVENT_RATE,
    );
    expect(screen.getByTestId("pricing-flow-cell-stage-1")).toHaveTextContent(
      RATES_DISPLAY.STAGE_PLATFORM,
    );
    expect(screen.getByTestId("pricing-flow-cell-stage-2")).toHaveTextContent(
      RATES_DISPLAY.STAGE_PLATFORM,
    );
    expect(screen.getByTestId("pricing-flow-cell-stage-n")).toHaveTextContent(
      RATES_DISPLAY.STAGE_PLATFORM,
    );
  });

  it("renders the LLM-stratum stating the user's provider keeps a separate bill", () => {
    renderPricing();
    const llm = screen.getByTestId("pricing-flow-llm");
    expect(llm).toBeInTheDocument();
    expect(llm.className).toMatch(/border-dashed/);
    expect(llm).toHaveTextContent(/not on your usezombie bill/i);
    expect(llm).toHaveTextContent(/your provider/i);
    expect(llm).toHaveTextContent(/Anthropic.*OpenAI.*Fireworks.*Together.*Groq.*Moonshot/);
  });

  it("does not render the old onboarding 4-step (Install/Wake/Reason/Evidence cards)", () => {
    renderPricing();
    expect(screen.queryByTestId("pricing-steps")).not.toBeInTheDocument();
    expect(screen.queryByRole("heading", { level: 3, name: "Install" })).not.toBeInTheDocument();
    expect(screen.queryByRole("heading", { level: 3, name: "Wake on event" })).not.toBeInTheDocument();
    expect(screen.queryByRole("heading", { level: 3, name: "Evidence" })).not.toBeInTheDocument();
  });

  it("renders the operational-extras list as 'per workspace, not gated by tier'", () => {
    renderPricing();
    expect(
      screen.getByText(/operational extras — provisioned per workspace as you scale, not gated by tier/i),
    ).toBeInTheDocument();
    expect(screen.getByText(/multi-workspace with shared event history/i)).toBeInTheDocument();
    expect(screen.getByText(/approval gating in dashboard and Slack DM/i)).toBeInTheDocument();
    expect(screen.getByText(/workspace-scoped credentials and webhooks/i)).toBeInTheDocument();
    expect(
      screen.getByText(/higher concurrency and longer per-stage windows — lift caps on request/i),
    ).toBeInTheDocument();
    expect(screen.getByText(/priority support/i)).toBeInTheDocument();
  });

  it("renders a single install CTA pointing at APP_BASE_URL", () => {
    renderPricing();
    const cta = screen.getByTestId("pricing-install-cta");
    expect(cta).toHaveAttribute("href", "https://app.dev.usezombie.com");
    expect(cta.textContent).toMatch(/install/i);
    expect(screen.queryByRole("link", { name: /upgrade/i })).not.toBeInTheDocument();
  });

  it("install CTA hugs its content (self-start) instead of stretching to card width", () => {
    renderPricing();
    const cta = screen.getByTestId("pricing-install-cta");
    // The Slot composes the Button's classes onto the <a>. self-start
    // breaks the flex-col Card's default `align-items: stretch`.
    expect(cta.className).toMatch(/\bself-start\b/);
  });

  it("install CTA fires trackSignupStarted (NOT signupCompleted — funnel hygiene) with pricing_install source", () => {
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
    expect(screen.queryByTestId("pricing-price-hobby")).not.toBeInTheDocument();
    expect(screen.queryByTestId("pricing-price-scale")).not.toBeInTheDocument();
  });
});
