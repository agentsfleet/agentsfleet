import { fireEvent, render, screen } from "@testing-library/react";
import { BrowserRouter } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";

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

  it("renders the rate line with $0.01 per event and $0.10 per stage", () => {
    renderPricing();
    expect(screen.getByTestId("pricing-rate-event")).toHaveTextContent("$0.01");
    expect(screen.getByTestId("pricing-rate-stage")).toHaveTextContent("$0.10");
    const line = screen.getByTestId("pricing-rate-line");
    expect(line).toHaveTextContent(/per event receipt/i);
    expect(line).toHaveTextContent(/per stage execution/i);
  });

  it("renders the $5 starter credit badge", () => {
    renderPricing();
    expect(screen.getByText(/\$5 starter credit, never expires/i)).toBeInTheDocument();
  });

  it("renders the worked example: 100 × $0.01 + 300 × $0.10 = $31.00", () => {
    renderPricing();
    const ex = screen.getByTestId("pricing-worked-example");
    expect(ex).toHaveTextContent(/100 × \$0\.01/);
    expect(ex).toHaveTextContent(/300 × \$0\.10/);
    expect(ex).toHaveTextContent(/\$31\.00/);
  });

  it("renders the BYOK provider list with no markup", () => {
    renderPricing();
    expect(
      screen.getByText(/BYOK on Anthropic, OpenAI, Fireworks, Together, Groq, Moonshot/i),
    ).toBeInTheDocument();
    expect(screen.getByText(/never marks up inference/i)).toBeInTheDocument();
    expect(screen.getByText(/usezombie marks up zero on inference/i)).toBeInTheDocument();
  });

  it("explains what a stage is in plain language", () => {
    renderPricing();
    const card = screen.getByTestId("pricing-rate-card");
    expect(card.textContent).toMatch(/stage is one reasoning step/i);
    expect(card.textContent).toMatch(/most diagnoses resolve in 1.{0,3}5 stages/i);
  });

  it("renders the stealth-mode banner with design-partner contact", () => {
    renderPricing();
    const note = screen.getByTestId("pricing-design-partner-note");
    expect(note).toHaveTextContent(/stealth-mode testing/i);
    expect(note).toHaveTextContent(/design partner/i);
    expect(note.querySelector("a")).toHaveAttribute(
      "href",
      expect.stringContaining("usezombie@agentmail.to"),
    );
  });

  it("renders the billing flow diagram with one event + three stage cells", () => {
    renderPricing();
    const flow = screen.getByTestId("pricing-flow");
    expect(flow).toBeInTheDocument();
    const billed = screen.getByTestId("pricing-flow-billed");
    const cells = billed.querySelectorAll('[data-testid^="pricing-flow-cell-"]');
    expect(cells).toHaveLength(4);
    expect(screen.getByTestId("pricing-flow-cell-event")).toHaveTextContent(/\$0\.01/);
    expect(screen.getByTestId("pricing-flow-cell-stage-1")).toHaveTextContent(/\$0\.10/);
    expect(screen.getByTestId("pricing-flow-cell-stage-2")).toHaveTextContent(/\$0\.10/);
    expect(screen.getByTestId("pricing-flow-cell-stage-n")).toHaveTextContent(/\$0\.10/);
  });

  it("renders the LLM-stratum stating BYOK is on a separate bill", () => {
    renderPricing();
    const llm = screen.getByTestId("pricing-flow-llm");
    expect(llm).toBeInTheDocument();
    expect(llm.className).toMatch(/border-dashed/);
    expect(llm).toHaveTextContent(/not on your usezombie bill/i);
    expect(llm).toHaveTextContent(/BYOK/);
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
