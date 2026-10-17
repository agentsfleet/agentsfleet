import { fireEvent, render, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { PUBLIC_ACCESS_URL } from "../config";
import { SUPPORT_EMAIL } from "../lib/contact";
import Pricing from "./Pricing";

const analytics = vi.hoisted(() => ({
  trackNavigationClicked: vi.fn(),
  trackSignupStarted: vi.fn(),
}));
vi.mock("../analytics/posthog", () => analytics);

describe("Getting started and pricing", () => {
  beforeEach(() => {
    analytics.trackNavigationClicked.mockReset();
    analytics.trackSignupStarted.mockReset();
  });

  it("states the approved starting price, included credit, and own-key billing", () => {
    render(<Pricing />);
    const section = screen.getByTestId("pricing-block");
    expect(screen.getByTestId("pricing-early-access-banner")).toHaveTextContent("$0 to start · $25 credit included");
    expect(section).toHaveTextContent("Run your first jobs with $25 in included credit.");
    expect(section).toHaveTextContent("If you bring your own key, usage is billed by your provider directly.");
    expect(section).not.toHaveTextContent(/early access|model credit|unlimited|zero markup|save \d/i);
    expect(within(section).queryByRole("link", { name: /buy|purchase|subscribe|upgrade/i })).toBeNull();
    expect(screen.queryByTestId("pricing-card-enterprise")).toBeNull();
  });

  it("opens hosted sign-up and preserves existing attribution", () => {
    render(<Pricing />);
    const action = screen.getByTestId("pricing-cta-early-access");
    expect(action).toHaveAttribute("href", PUBLIC_ACCESS_URL);
    expect(action).toHaveAttribute("target", "_blank");
    expect(action).toHaveAttribute("rel", "noopener noreferrer");
    expect(action).toHaveTextContent(/Join waitlist/);
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
