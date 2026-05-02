import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";

vi.mock("lucide-react", () => ({}));

import BillingBalanceCard from "@/app/(dashboard)/settings/billing/components/BillingBalanceCard";
import type { TenantBilling } from "@/lib/types";

const HEALTHY: TenantBilling = {
  plan_tier: "free",
  plan_sku: "starter",
  balance_cents: 471,
  updated_at: 1,
  is_exhausted: false,
  exhausted_at: null,
};

afterEach(() => cleanup());

describe("BillingBalanceCard", () => {
  it("renders formatted balance + subtitle for a healthy tenant", () => {
    render(React.createElement(BillingBalanceCard, { billing: HEALTHY }));
    expect(screen.getByText(/\$4\.71/)).toBeTruthy();
    expect(screen.getByText("Covers all your zombie events.")).toBeTruthy();
  });

  it("renders a disabled Purchase Credits button (Stripe deferred to v2.1)", () => {
    render(React.createElement(BillingBalanceCard, { billing: HEALTHY }));
    const btn = screen.getByRole("button", { name: /purchase credits/i }) as HTMLButtonElement;
    expect(btn.disabled).toBe(true);
    expect(btn.getAttribute("aria-disabled")).toBe("true");
  });

  it("surfaces an alert banner when the balance is exhausted", () => {
    const exhausted: TenantBilling = { ...HEALTHY, balance_cents: 0, is_exhausted: true };
    render(React.createElement(BillingBalanceCard, { billing: exhausted }));
    const alert = screen.getByRole("alert");
    expect(alert.textContent).toMatch(/Balance exhausted/);
    expect(alert.textContent).toMatch(/Stripe purchase ships in v2\.1/);
  });

  it("applies destructive treatment to the balance headline when exhausted", () => {
    const exhausted: TenantBilling = { ...HEALTHY, balance_cents: 0, is_exhausted: true };
    render(React.createElement(BillingBalanceCard, { billing: exhausted }));
    const headline = screen.getByTestId("balance-headline");
    expect(headline.getAttribute("data-exhausted")).toBe("true");
    expect(headline.className).toContain("text-destructive");
  });

  it("does NOT apply destructive treatment when the balance is healthy", () => {
    render(React.createElement(BillingBalanceCard, { billing: HEALTHY }));
    const headline = screen.getByTestId("balance-headline");
    expect(headline.getAttribute("data-exhausted")).toBe("false");
    expect(headline.className).not.toContain("text-destructive");
  });

  it("Purchase Credits trigger is keyboard-reachable (a11y)", () => {
    render(React.createElement(BillingBalanceCard, { billing: HEALTHY }));
    const trigger = screen.getByTestId("purchase-credits-trigger");
    // Wrapper carries tabIndex={0}; the disabled button inside is removed
    // from the tab order via tabIndex={-1} so screen-reader users land on
    // the trigger and hear the "Coming in v2.1" tooltip via aria-describedby.
    expect(trigger.getAttribute("tabindex")).toBe("0");
    expect(trigger.getAttribute("aria-describedby")).toBe("purchase-credits-tooltip");
  });
});
