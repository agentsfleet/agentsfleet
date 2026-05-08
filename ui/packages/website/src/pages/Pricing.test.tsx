import { fireEvent, render, screen } from "@testing-library/react";
import { BrowserRouter } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";

const analytics = vi.hoisted(() => ({
  trackLeadCaptureClicked: vi.fn(),
  trackSignupCompleted: vi.fn(),
}));

vi.mock("../analytics/posthog", async () => {
  const actual = await vi.importActual<typeof import("../analytics/posthog")>("../analytics/posthog");
  return {
    ...actual,
    trackLeadCaptureClicked: analytics.trackLeadCaptureClicked,
    trackSignupCompleted: analytics.trackSignupCompleted,
  };
});

import Pricing from "./Pricing";

function renderPricing() {
  return render(
    <BrowserRouter>
      <Pricing />
    </BrowserRouter>
  );
}

describe("Pricing", () => {
  beforeEach(() => {
    analytics.trackLeadCaptureClicked.mockReset();
    analytics.trackSignupCompleted.mockReset();
  });

  it("renders the heading", () => {
    renderPricing();
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent(
      /start free\. upgrade when you need stronger control\./i,
    );
  });

  it("renders roadmap proof points", () => {
    renderPricing();
    expect(screen.getByText(/byok with no token markup/i)).toBeInTheDocument();
    expect(screen.getByText(/open source runtime/i)).toBeInTheDocument();
    expect(screen.getByText(/three triggers, one reasoning loop/i)).toBeInTheDocument();
    expect(screen.getByText(/self-host arrives in v3/i)).toBeInTheDocument();
  });

  it("renders Hobby and Scale tiers", () => {
    renderPricing();
    expect(screen.getByRole("heading", { level: 2, name: "Hobby" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { level: 2, name: "Scale" })).toBeInTheDocument();
  });

  it("renders the Hobby start CTA", () => {
    renderPricing();
    expect(screen.getByRole("link", { name: /start free/i })).toHaveAttribute(
      "href",
      "https://app.dev.usezombie.com",
    );
  });

  it("shows Free price and lowercase availability badges", () => {
    renderPricing();
    expect(screen.getByTestId("pricing-price-hobby")).toHaveTextContent("Free");
    expect(screen.getByText(/^available now$/)).toBeInTheDocument();
    expect(screen.getByText(/^upgrade when ready$/)).toBeInTheDocument();
  });

  it("renders Scale upgrade copy and CTA", () => {
    renderPricing();
    expect(screen.getByRole("link", { name: /upgrade in app/i })).toHaveAttribute(
      "href",
      "https://app.dev.usezombie.com",
    );
    expect(
      screen.getByText(/operator-visible upgrade path after free credit exhaustion/i),
    ).toBeInTheDocument();
    expect(screen.getByText(/workspace-scoped credentials and webhooks/i)).toBeInTheDocument();
  });

  it("renders move-up guidance copy", () => {
    renderPricing();
    expect(screen.getByText(/start on hobby/i)).toBeInTheDocument();
  });

  it("renders FAQ section", () => {
    renderPricing();
    expect(screen.getByText("What does BYOK mean?")).toBeInTheDocument();
  });

  it("renders all highlight bullet points for both tiers", () => {
    renderPricing();
    expect(screen.getByText("$5 starter credit, never expires")).toBeInTheDocument();
    expect(screen.getByText("1 workspace")).toBeInTheDocument();
    expect(
      screen.getByText("BYOK on Anthropic, OpenAI, Fireworks (Kimi K2), Together, Groq, Moonshot"),
    ).toBeInTheDocument();
    expect(screen.getByText("Everything in Hobby")).toBeInTheDocument();
    expect(screen.getByText("Multiple workspaces with shared event history")).toBeInTheDocument();
    expect(screen.getByText("Workspace-scoped credentials and webhooks")).toBeInTheDocument();
    expect(screen.getByText("Priority support")).toBeInTheDocument();
  });

  it("flags the Scale tier with a pulse-bordered featured card", () => {
    renderPricing();
    const hobby = screen.getByTestId("pricing-card-hobby");
    const scale = screen.getByTestId("pricing-card-scale");
    expect(hobby.getAttribute("data-featured")).toBeNull();
    expect(scale.getAttribute("data-featured")).toBe("true");
    expect(scale.className).toMatch(/border-pulse/);
  });

  it("CTA links both point to APP_BASE_URL", () => {
    renderPricing();
    const hobbyLink = screen.getByRole("link", { name: /start free/i });
    const scaleLink = screen.getByRole("link", { name: /upgrade in app/i });
    expect(hobbyLink).toHaveAttribute("href", "https://app.dev.usezombie.com");
    expect(scaleLink).toHaveAttribute("href", "https://app.dev.usezombie.com");
  });

  it("tracks Hobby CTA clicks as signup completion", () => {
    renderPricing();
    fireEvent.click(screen.getByRole("link", { name: /start free/i }));
    expect(analytics.trackSignupCompleted).toHaveBeenCalledWith({
      source: "pricing_hobby_start_free",
      surface: "pricing",
      mode: "humans",
    });
  });

  it("tracks Scale CTA clicks as lead capture intent", () => {
    renderPricing();
    fireEvent.click(screen.getByRole("link", { name: /upgrade in app/i }));
    expect(analytics.trackLeadCaptureClicked).toHaveBeenCalledWith({
      page: "pricing",
      surface: "pricing_card",
      cta_id: "pricing_scale_upgrade",
      plan_interest: "Scale",
    });
  });

  it("renders price note only for Scale tier", () => {
    renderPricing();
    expect(
      screen.getByText("Operator-visible upgrade path after free credit exhaustion"),
    ).toBeInTheDocument();
  });
});
