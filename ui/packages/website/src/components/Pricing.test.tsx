import { fireEvent, render, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { WAITLIST_URL } from "../config";
import { SUPPORT_EMAIL } from "../lib/contact";
import { PRICING_COPY } from "../lib/marketing-copy";
import Pricing from "./Pricing";

const analytics = vi.hoisted(() => ({
  trackNavigationClicked: vi.fn(),
  trackSignupStarted: vi.fn(),
}));
vi.mock("../analytics/posthog", () => analytics);

describe("Early access and pricing", () => {
  beforeEach(() => {
    analytics.trackNavigationClicked.mockReset();
    analytics.trackSignupStarted.mockReset();
  });

  it("invites workflow feedback before pricing is decided", () => {
    render(<Pricing />);
    expect(screen.getByTestId("pricing-block")).toHaveTextContent(PRICING_COPY.lede);
    expect(screen.getByTestId("pricing-early-access-banner")).toHaveTextContent(/pricing is being worked out/i);
  });

  it("does not publish speculative prices, credits, savings, or free usage", () => {
    render(<Pricing />);
    const section = screen.getByTestId("pricing-block");
    expect(section).not.toHaveTextContent(/\$\d|starter credit|start free|unlimited|zero markup|save \d/i);
    expect(within(section).queryByRole("link", { name: /buy|purchase|subscribe|upgrade/i })).toBeNull();
    expect(screen.queryByTestId("pricing-card-enterprise")).toBeNull();
  });

  it("does not imply a model key removes runtime costs", () => {
    render(<Pricing />);
    expect(screen.getByTestId("pricing-block")).toHaveTextContent(/does not remove fleet runtime costs/i);
    expect(screen.getByTestId("pricing-block")).toHaveTextContent(/model usage is separate/i);
  });

  it("requires pricing and access terms to be confirmed before use", () => {
    render(<Pricing />);
    expect(screen.getByTestId("pricing-block")).toHaveTextContent(PRICING_COPY.note);
  });

  it("uses a waitlist link rather than a purchase action and preserves attribution", () => {
    render(<Pricing />);
    const action = screen.getByTestId("pricing-cta-early-access");
    expect(action).toHaveAttribute("href", WAITLIST_URL);
    expect(action).toHaveAttribute("target", "_blank");
    expect(action).toHaveAttribute("rel", "noopener noreferrer");
    expect(action).toHaveTextContent(/request early access/i);
    fireEvent.click(action);
    expect(analytics.trackSignupStarted).toHaveBeenCalledWith({
      source: "pricing_early_access", surface: "pricing", mode: "humans",
    });
  });

  it("provides a direct feedback path without pretending an enterprise plan exists", () => {
    render(<Pricing />);
    const link = screen.getByRole("link", { name: /tell us about your workflow/i });
    expect(link).toHaveAttribute("href", `mailto:${SUPPORT_EMAIL}`);
    expect(link).not.toHaveAttribute("target");
    fireEvent.click(link);
    expect(analytics.trackNavigationClicked).toHaveBeenCalledWith({
      source: "pricing_contact", surface: "pricing", target: "email",
    });
  });
});
