import { afterEach, expect, it, vi } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import { TooltipProvider } from "@agentsfleet/design-system";
import { BillingDailySpend } from "@/app/(dashboard)/settings/billing/components/BillingDailySpend";
import { BILLING_QUERY, BILLING_SETUP } from "@/app/(dashboard)/settings/billing/lib/report-query";
import { billingReport, billingTotals } from "./helpers/billing-report";

const { selected } = vi.hoisted(() => ({ selected: { params: new URLSearchParams() } }));
vi.mock("next/navigation", () => ({ useSearchParams: () => selected.params }));
afterEach(() => { cleanup(); selected.params = new URLSearchParams(); });

it("renders complete daily amounts independently of the activity page and scales exact nanos", () => {
  const large = "9007199254741049999";
  const daily = [
    { day: "2026-10-24", totals: billingTotals({charged_nanos: "0"}) },
    { day: "2026-10-25", totals: billingTotals({charged_nanos: large}) },
  ];
  render(<BillingDailySpend report={billingReport({daily, activities: {items: [], total: 100, next_cursor: "more"}})} />, {wrapper: TooltipProvider});
  const chart = screen.getByRole("list", {name: "Daily settled charges"});
  expect(within(chart).getByText("2026-10-24 UTC: $0.00")).toBeTruthy();
  expect(within(chart).getByText("2026-10-25 UTC: $9,007,199,254.741")).toBeTruthy();
  expect(screen.getByText("Daily high $9,007,199,254.741")).toBeTruthy();
  expect(Array.from(chart.querySelectorAll<HTMLElement>('[aria-hidden="true"]'), bar => bar.style.height)).toEqual(["0%", "100%"]);
});

it("keeps a true zero distinct from missing usage and names the selected deleted fleet", () => {
  selected.params = new URLSearchParams("fleet=deleted-fleet");
  render(<BillingDailySpend report={billingReport({daily: [{day: "2026-10-25", totals: billingTotals({charged_nanos: "0"})}]})} />, {wrapper: TooltipProvider});
  expect(screen.getByText("No settled charges for this period.")).toBeTruthy();
  expect(screen.getByText("Daily spend · deleted-fleet")).toBeTruthy();
  expect(screen.getByText("Daily high $0.00")).toBeTruthy();
});

it("labels setup-only charges without attributing them to a fleet", () => {
  selected.params = new URLSearchParams({ [BILLING_QUERY.fleet]: BILLING_SETUP });
  const totals = billingTotals({charged_nanos: "7000000000"});
  render(<BillingDailySpend report={billingReport({from_at: "2026-10-25T00:00:00Z", totals})} />, {wrapper: TooltipProvider});
  expect(screen.getByText("Daily spend · Setup chat")).toBeTruthy();
  const chart = screen.getByRole("list", {name: "Daily settled charges"});
  expect(within(chart).getByText("2026-10-25 UTC: $7.00")).toBeTruthy();
  expect(screen.getByText("Daily high $7.00")).toBeTruthy();
});
