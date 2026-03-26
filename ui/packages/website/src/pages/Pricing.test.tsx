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
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent(/start free\. upgrade when you need stronger control\./i);
  });

  it("renders roadmap proof points", () => {
    renderPricing();
    expect(screen.getByText(/run quality scoring and failure analysis/i)).toBeInTheDocument();
    expect(screen.getByText(/sandbox governance and team controls/i)).toBeInTheDocument();
  });

  it("renders Hobby and Scale tiers", () => {
    renderPricing();
    expect(screen.getByRole("heading", { level: 2, name: "Hobby" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { level: 2, name: "Scale" })).toBeInTheDocument();
  });

  it("renders Start free CTA for Hobby", () => {
    renderPricing();
    expect(screen.getByRole("link", { name: /start free/i })).toHaveAttribute(
      "href",
      "https://app.dev.usezombie.com",
    );
  });

  it("shows Free and direct-upgrade availability in the card chrome", () => {
    renderPricing();
    expect(screen.getByText("Free")).toBeInTheDocument();
    expect(screen.getByText("Upgrade when ready")).toBeInTheDocument();
  });

  it("renders Scale upgrade copy and CTA", () => {
    renderPricing();
    expect(screen.getByRole("link", { name: /upgrade in app/i })).toHaveAttribute(
      "href",
      "https://app.dev.usezombie.com",
    );
    expect(screen.getByText(/operator-visible upgrade path after free credit exhaustion/i)).toBeInTheDocument();
    expect(screen.getByText(/workspace subscription id/i)).toBeInTheDocument();
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
    // Hobby highlights
    expect(screen.getByText("$10 credit included with no expiry")).toBeInTheDocument();
    expect(screen.getByText("1 workspace and automated pull request generation")).toBeInTheDocument();
    expect(screen.getByText("Harness validation in the PR flow")).toBeInTheDocument();
    expect(screen.getByText("BYOK/BYOM with no token markup")).toBeInTheDocument();
    // Scale highlights
    expect(screen.getByText("Everything in Hobby, plus team workspaces and shared run history")).toBeInTheDocument();
    expect(screen.getByText("Longer execution windows and higher concurrency for active repos")).toBeInTheDocument();
    expect(screen.getByText("Sandbox resource governance with memory, CPU, and disk caps")).toBeInTheDocument();
    expect(screen.getByText("Agent run scoring with tier history and workspace baselines")).toBeInTheDocument();
    expect(screen.getByText("Failure analysis with deterministic context injection")).toBeInTheDocument();
    expect(screen.getByText("Upgrade from the app with a workspace subscription id")).toBeInTheDocument();
  });

  it("applies featured card class only to Scale tier", () => {
    renderPricing();
    const articles = screen.getAllByRole("article");
    const hobbyCard = articles.find((el) => el.querySelector("h2")?.textContent === "Hobby")!;
    const scaleCard = articles.find((el) => el.querySelector("h2")?.textContent === "Scale")!;
    expect(hobbyCard.className).not.toMatch(/pricing-card--featured/);
    expect(scaleCard.className).toMatch(/pricing-card--featured/);
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
    const articles = screen.getAllByRole("article");
    const hobbyCard = articles.find((el) => el.querySelector("h2")?.textContent === "Hobby")!;
    const scaleCard = articles.find((el) => el.querySelector("h2")?.textContent === "Scale")!;
    expect(hobbyCard.querySelector(".pricing-card-note")).toBeNull();
    expect(scaleCard.querySelector(".pricing-card-note")).not.toBeNull();
    expect(scaleCard.querySelector(".pricing-card-note")!.textContent).toBe(
      "Operator-visible upgrade path after free credit exhaustion",
    );
  });

  it("renders both pricing cards as article elements", () => {
    renderPricing();
    const articles = screen.getAllByRole("article");
    expect(articles).toHaveLength(2);
  });
});
