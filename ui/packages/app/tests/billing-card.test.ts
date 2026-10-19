import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { TooltipProvider } from "@agentsfleet/design-system";

vi.mock("lucide-react", () => ({
  CoinsIcon: () => React.createElement("svg", { "data-icon": "CoinsIcon" }),
}));

import BillingBalanceCard from "@/app/(dashboard)/settings/billing/components/BillingBalanceCard";
import type { TenantBilling } from "@/lib/types";

const HEALTHY: TenantBilling = {
  balance_nanos: "4710000000",
  updated_at: "1970-01-01T00:00:00.001Z",
  is_exhausted: false,
  exhausted_at: null,
};

// The wrapper stands in for a real ancestor: the root layout mounts the app's
// one TooltipProvider, and this card's Buy-credits tooltip reads it from there.
function renderCard(billing: TenantBilling) {
  return render(React.createElement(BillingBalanceCard, { billing }), {
    wrapper: TooltipProvider,
  });
}

afterEach(() => cleanup());

describe("BillingBalanceCard", () => {
  it("renders the formatted balance + USD unit", () => {
    renderCard(HEALTHY);
    expect(screen.getByText(/\$4\.71/)).toBeTruthy();
    expect(screen.getByText("USD")).toBeTruthy();
  });

  it("does not present a page of charges as a balance percentage", () => {
    renderCard(HEALTHY);
    expect(screen.queryByTestId("balance-meter")).toBeNull();
    expect(screen.queryByText(/spent/i)).toBeNull();
    expect(screen.getByText("Available credits")).toBeTruthy();
  });

  it("renders Buy credits as a live mailto link, not a disabled button", () => {
    renderCard(HEALTHY);
    const link = screen.getByRole("link", { name: /buy credits/i }) as HTMLAnchorElement;
    expect(link.getAttribute("href")).toBe("mailto:agentsfleet@agentmail.to");
    expect(link.hasAttribute("disabled")).toBe(false);
    expect(link.getAttribute("aria-disabled")).toBeNull();
  });

  it.each([["0", "$0.00"], ["-50000000", "-$0.05"], ["-50", ">-$0.0001"]])(
    "shows exhausted balance %s and a top-up alert", (balance_nanos, expected) => {
    renderCard({ ...HEALTHY, balance_nanos, is_exhausted: true });
    expect(screen.getByTestId("balance-headline").textContent).toBe(expected);
    const alert = screen.getByRole("alert");
    expect(alert.textContent).toMatch(/Balance exhausted/);
    expect(alert.textContent).toMatch(/top up/i);
  });

  it("applies destructive treatment to the balance headline when exhausted", () => {
    renderCard({ ...HEALTHY, balance_nanos: "0", is_exhausted: true });
    const headline = screen.getByTestId("balance-headline");
    expect(headline.getAttribute("data-exhausted")).toBe("true");
    expect(headline.className).toContain("text-destructive");
  });

  it("does NOT apply destructive treatment when the balance is healthy", () => {
    renderCard(HEALTHY);
    const headline = screen.getByTestId("balance-headline");
    expect(headline.getAttribute("data-exhausted")).toBe("false");
    expect(headline.className).not.toContain("text-destructive");
  });

  it("Buy credits trigger is a real anchor — natively keyboard-reachable, tooltip-described", () => {
    renderCard(HEALTHY);
    const trigger = screen.getByTestId("buy-credits-trigger");
    expect(trigger.tagName).toBe("A");
    expect(trigger.getAttribute("aria-describedby")).toBe("buy-credits-tooltip");
  });

  it("renders the support email link using SUPPORT_EMAIL when exhausted", () => {
    renderCard({ ...HEALTHY, balance_nanos: "0", is_exhausted: true });
    const link = screen.getByRole("link", { name: /support/i }) as HTMLAnchorElement;
    expect(link.getAttribute("href")).toBe("mailto:agentsfleet@agentmail.to");
  });
});
