import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";

vi.mock("lucide-react", () => ({
  CoinsIcon: () => React.createElement("svg", { "data-icon": "CoinsIcon" }),
}));

import BillingBalanceCard from "@/app/(dashboard)/settings/billing/components/BillingBalanceCard";
import type { ChargeSummary } from "@/app/(dashboard)/settings/billing/lib/charges";
import type { TenantBilling } from "@/lib/types";

const HEALTHY: TenantBilling = {
  balance_nanos: 4_710_000_000,
  updated_at: 1,
  is_exhausted: false,
  exhausted_at: null,
  free_trial: { active: false, ends_at_ms: 1_785_542_400_000 },
};

const SUMMARY: ChargeSummary = { spentNanos: 290_000_000, eventCount: 4, meterPct: 6 };

function renderCard(billing: TenantBilling, summary: ChargeSummary = SUMMARY) {
  return render(React.createElement(BillingBalanceCard, { billing, summary }));
}

afterEach(() => cleanup());

describe("BillingBalanceCard", () => {
  it("renders the formatted balance + USD unit", () => {
    renderCard(HEALTHY);
    expect(screen.getByText(/\$4\.71/)).toBeTruthy();
    expect(screen.getByText("USD")).toBeTruthy();
  });

  // test_billing_balance_layout — amount + full-width meter + caption + header
  // CTA all present; the meter fills the row so the CTA is not stranded.
  it("test_billing_balance_layout: amount, full-width meter, caption, and header CTA all render", () => {
    renderCard(HEALTHY);
    // amount
    expect(screen.getByTestId("balance-headline").textContent).toMatch(/\$4\.71/);
    // full-width meter, filled to the summary percentage
    const meter = screen.getByTestId("balance-meter");
    const fill = meter.querySelector("span") as HTMLSpanElement;
    expect(fill.style.width).toBe("6%");
    // caption: spent + events ride the meter's end
    expect(screen.getByTestId("balance-usage").textContent).toMatch(/spent\s*\$0\.29\s*·\s*4\s*events/);
    // header CTA present (in the head row, not a stranded control)
    expect(screen.getByTestId("buy-credits-trigger")).toBeTruthy();
  });

  it("singularizes the event caption when exactly one event", () => {
    renderCard(HEALTHY, { spentNanos: 30_000_000, eventCount: 1, meterPct: 1 });
    const usage = screen.getByTestId("balance-usage").textContent ?? "";
    expect(usage).toMatch(/·\s*1\s*event$/);
    expect(usage).not.toMatch(/events/);
  });

  it("renders Buy credits as a live mailto link, not a disabled button", () => {
    renderCard(HEALTHY);
    const link = screen.getByRole("link", { name: /buy credits/i }) as HTMLAnchorElement;
    expect(link.getAttribute("href")).toBe("mailto:agentsfleet@agentmail.to");
    expect(link.hasAttribute("disabled")).toBe(false);
    expect(link.getAttribute("aria-disabled")).toBeNull();
  });

  it("surfaces an alert banner when the balance is exhausted", () => {
    renderCard({ ...HEALTHY, balance_nanos: 0, is_exhausted: true });
    const alert = screen.getByRole("alert");
    expect(alert.textContent).toMatch(/Balance exhausted/);
    expect(alert.textContent).toMatch(/top up/i);
  });

  it("applies destructive treatment to the balance headline when exhausted", () => {
    renderCard({ ...HEALTHY, balance_nanos: 0, is_exhausted: true });
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
    renderCard({ ...HEALTHY, balance_nanos: 0, is_exhausted: true });
    const link = screen.getByRole("link", { name: /support/i }) as HTMLAnchorElement;
    expect(link.getAttribute("href")).toBe("mailto:agentsfleet@agentmail.to");
  });
});
