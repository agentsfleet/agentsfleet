import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { TooltipProvider } from "@agentsfleet/design-system";
import { BillingReportView } from "@/app/(dashboard)/settings/billing/components/BillingReportView";
import { BillingSpending } from "@/app/(dashboard)/settings/billing/components/BillingSpending";
import { BILLING_MODEL, BILLING_MODEL_NAME, billingReport, billingRowTotals, billingTotals } from "./helpers/billing-report";

const { push, current } = vi.hoisted(() => ({ push: vi.fn(), current: { params: new URLSearchParams() } }));
vi.mock("next/navigation", () => ({
  useRouter: () => ({ push }), usePathname: () => "/settings/billing", useSearchParams: () => current.params,
}));

const names = { [BILLING_MODEL]: BILLING_MODEL_NAME };
const BY_FLEET_LABEL = "Charged by fleet or setup chat";
const BY_MODEL_LABEL = "Charged by model";
function view(report = billingReport()) {
  return render(<BillingReportView report={report} modelNames={names} />, { wrapper: TooltipProvider });
}
async function choose(label: string, value: string) {
  const trigger = screen.getByRole("combobox", { name: label });
  fireEvent.click(trigger);
  fireEvent.keyDown(trigger, { key: "Enter" });
  fireEvent.click(screen.getByRole("option", { name: value }));
  await waitFor(() => expect(push).toHaveBeenCalled());
}
function pushedParams() {
  return new URL(String(push.mock.calls.at(-1)?.[0]), "https://example.test").searchParams;
}

beforeEach(() => { push.mockReset(); current.params = new URLSearchParams(); });
afterEach(() => cleanup());

describe("Billing report filters preserve period totals", () => {
  it("retains unattributed setup charges and a deleted fleet's identifier without inventing a name", () => {
    const report = billingReport();
    view({...report, spending: {...report.spending, by_fleet: [
      {key: null, label: null, models: [], totals: billingTotals()},
      {key: "deleted-fleet", label: null, models: ["retired-model"], totals: billingTotals()},
    ]}});
    const section = screen.getByRole("region", {name: "By fleet"});
    expect(within(section).getByText("Setup chats")).toBeTruthy();
    expect(within(section).getByText("Fleet setup")).toBeTruthy();
    expect(within(section).getByText("deleted-fleet")).toBeTruthy();
    expect(within(section).getByText("Fleet work")).toBeTruthy();
  });
  it("opens the previous UTC month and returns to the current month without losing fleet selection", () => {
    current.params = new URLSearchParams("fleet=deleted-fleet&through_at=2026-11-01T01:00:00Z");
    view(billingReport({from_at: "2026-11-01T00:00:00Z", through_at: "2026-11-01T01:00:00Z"}));
    fireEvent.click(screen.getByRole("button", {name: "More filters"}));
    fireEvent.click(screen.getByRole("button", {name: "Previous month"}));
    expect(pushedParams().get("through_at")).toBe("2026-10-31T23:59:59.999Z");
    expect(pushedParams().get("fleet")).toBe("deleted-fleet");
    fireEvent.click(screen.getByRole("button", {name: "Current month"}));
    expect(pushedParams().has("through_at")).toBe(false);
    expect(pushedParams().get("fleet")).toBe("deleted-fleet");
    expect(screen.getByText(/Billing history includes deleted fleets/)).toBeTruthy();
  });
  it("renders full-period totals with no activity list or charge details", () => {
    view(billingReport({ totals: billingTotals({ charged_nanos: "5200000000", activity_count: 52, line_count: 150 }) }));
    const totals = within(screen.getByLabelText("Period totals"));
    expect(totals.getByText("$5.20")).toBeTruthy();
    expect(totals.getByText("52")).toBeTruthy();
    expect(totals.getByText("Tokens")).toBeTruthy();
    expect(totals.getByText("50")).toBeTruthy();
    expect(screen.queryByRole("button", {name: "Charge details"})).toBeNull();
    expect(screen.queryByRole("table", {name: "Billing activity"})).toBeNull();
    expect(screen.getByText(/Totals exclude pending or unavailable provider usage/)).toBeTruthy();
  });

  it("an empty successful report has an honest empty state", () => {
    view(billingReport({ totals: billingTotals({ charged_nanos: "0", activity_count: 0, line_count: 0 }),
      daily: [{day: "2026-10-25", totals: billingRowTotals({charged_nanos: "0"})}],
      spending: { by_fleet: [], by_model: [] }, filters: { fleets: [], models: [] } }));
    expect(screen.getByText("No settled charges for this period.")).toBeTruthy();
    expect(screen.getAllByText("No settled spending")).toHaveLength(1);
  });

  it.each([["Period", "Last 7 days", "period", "week"], ["Period", "Today", "period", "today"], ["Fleet", "Setup chat", "fleet", "setup"]])(
    "changing %s retains the selected cutoff unless the period changes", async (label, choice, key, expected) => {
      current.params = new URLSearchParams("through_at=old");
      view();
      await choose(label, choice);
      const params = pushedParams();
      expect(params.get(key)).toBe(expected);
      expect(params.get("through_at")).toBe(label === "Period" ? null : "old");
    },
  );

  it("filter choices keep a deleted fleet selectable without duplicating private setup", async () => {
    const retained = "01924f4e-0000-7000-8000-00000000c4a9";
    view(billingReport({ filters: { fleets: [{ id: null, name: null }, { id: retained, name: null }], models: [] } }));
    const trigger = screen.getByRole("combobox", { name: "Fleet" });
    fireEvent.click(trigger);
    fireEvent.keyDown(trigger, { key: "Enter" });
    expect(screen.getAllByRole("option", { name: "Setup chat" })).toHaveLength(1);
    fireEvent.click(screen.getByRole("option", { name: retained }));
    await waitFor(() => expect(pushedParams().get("fleet")).toBe(retained));
  });

  it("offers no model filter, because the report no longer filters by model", () => {
    view();
    fireEvent.click(screen.getByRole("button", {name: "More filters"}));
    expect(screen.getByRole("button", {name: "Previous month"})).toBeTruthy();
    expect(screen.queryByRole("combobox", {name: "Model"})).toBeNull();
  });

  it("the spending view uses complete server groups and recorded names", () => {
    view();
    expect(within(screen.getByRole("list", { name: BY_FLEET_LABEL })).getByText("Payment alerts")).toBeTruthy();
    expect(within(screen.getByRole("list", { name: BY_MODEL_LABEL })).getByText(BILLING_MODEL_NAME)).toBeTruthy();
  });

  it("spending groups retain opaque labels and private setup when names are absent", () => {
    const totals = billingTotals();
    render(<BillingSpending modelNames={{}} report={billingReport({ spending: {
      by_fleet: [{ key: null, label: null, models: [], totals }, { key: "retained-fleet", label: null, models: [], totals }],
      by_model: [{ key: BILLING_MODEL, label: null, totals: billingRowTotals() }, { key: null, label: null, totals: billingRowTotals() }],
    } })} />, { wrapper: TooltipProvider });
    expect(screen.getByText("Setup chat")).toBeTruthy();
    expect(screen.getByText("retained-fleet")).toBeTruthy();
    expect(screen.getByText(BILLING_MODEL)).toBeTruthy();
    expect(screen.getByText("Unknown model")).toBeTruthy();
  });

  it("a zero-charge group keeps a zero bar and the other breakdown's empty state", () => {
    render(<BillingSpending modelNames={names} report={billingReport({ spending: {
      by_fleet: [{ key: null, label: null, models: [], totals: billingTotals({ charged_nanos: "0" }) }], by_model: [],
    } })} />);
    const group = screen.getByRole("list", { name: BY_FLEET_LABEL });
    expect(within(group).getByText("$0.00")).toBeTruthy();
    expect(group.querySelector<HTMLElement>(".usage-bar-fill")?.style.width).toBe("0%");
    expect(screen.getByText("No settled spending")).toBeTruthy();
  });

  it("preserves exact money and proportional bars above safe JavaScript integers", () => {
    render(<BillingSpending modelNames={names} report={billingReport({ spending: {
      by_fleet: [
        { key: "large", label: "Large charge", models: [], totals: billingTotals({ charged_nanos: "10000000000000000100000" }) },
        { key: "small", label: "Smaller charge", models: [], totals: billingTotals({ charged_nanos: "2500000000000000025000" }) },
      ], by_model: [],
    } })} />);
    const group = screen.getByRole("list", { name: BY_FLEET_LABEL });
    expect(within(group).getByText("$10,000,000,000,000.0001")).toBeTruthy();
    const bars = group.querySelectorAll<HTMLElement>(".usage-bar-fill");
    expect(Array.from(bars, bar => bar.style.width)).toEqual(["100%", "25%"]);
  });
});
