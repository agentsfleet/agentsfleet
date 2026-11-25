import { fireEvent, render, screen, within } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { PUBLIC_ACCESS_URL } from "../config";
import { Pricing } from "./Pricing";

const analytics = vi.hoisted(() => ({
  trackNavigationClicked: vi.fn(),
  trackSignupStarted: vi.fn(),
}));
vi.mock("../analytics/posthog", () => analytics);

it("states the approved starting price and own-key billing", () => {
  render(<Pricing waitlistUrl={PUBLIC_ACCESS_URL} />);
  const section = screen.getByTestId("pricing-block");
  expect(section).toHaveTextContent("Fleet runtime starts at $0.");
  expect(section).toHaveTextContent("If you bring your own model key, your provider bills model usage directly.");
  expect(section).not.toHaveTextContent(/included credit|credit included|\$25|unlimited|zero markup/i);
  expect(within(section).queryByRole("link", { name: /buy|purchase|subscribe|upgrade/i })).toBeNull();
});

it("links to the waitlist and terminal guide with attribution", () => {
  render(<Pricing waitlistUrl={PUBLIC_ACCESS_URL} />);
  const signup = screen.getByRole("link", { name: "Join waitlist" });
  expect(signup).toHaveAttribute("href", PUBLIC_ACCESS_URL);
  expect(signup).toHaveAttribute("target", "_blank");
  expect(signup).toHaveAttribute("rel", "noopener noreferrer");
  fireEvent.click(signup);
  expect(analytics.trackSignupStarted).toHaveBeenCalledWith({ source: "pricing_signup", surface: "pricing", mode: "humans" });
  const guide = screen.getByRole("link", { name: "Use the terminal" });
  expect(guide).toHaveAttribute("href", "/agents");
  fireEvent.click(guide);
  expect(analytics.trackNavigationClicked).toHaveBeenCalledWith({ source: "pricing_terminal", surface: "pricing", target: "agents" });
});
