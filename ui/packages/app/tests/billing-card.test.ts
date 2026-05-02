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
});
