import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { TooltipProvider } from "@agentsfleet/design-system";
import { BillingReportView } from "@/app/(dashboard)/settings/billing/components/BillingReportView";
import { BILLING_AT, BILLING_FLEET, BILLING_MODEL, BILLING_MODEL_NAME, billingReport, billingTotals } from "./helpers/billing-report";

const { push, current } = vi.hoisted(() => ({ push: vi.fn(), current: { params: new URLSearchParams() } }));
vi.mock("next/navigation", () => ({
  useRouter: () => ({ push }), usePathname: () => "/settings/billing", useSearchParams: () => current.params,
}));
const names: Record<string, string> = { [BILLING_MODEL]: BILLING_MODEL_NAME };
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
beforeEach(() => { vi.useFakeTimers({toFake: ["Date"]}); vi.setSystemTime(new Date("2026-11-15T12:00:00Z")); push.mockReset(); current.params = new URLSearchParams(); });
afterEach(() => {cleanup(); vi.useRealTimers();});

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
  it("selects named months and returns to the current report without losing fleet selection", async () => {
    current.params = new URLSearchParams("fleet=deleted-fleet&through_at=2026-11-01T01:00:00Z&c=old&activity=old");
    const rendered = view(billingReport({from_at: "2026-11-01T00:00:00Z", through_at: "2026-11-01T01:00:00Z"}));
    await choose("Period", "October 2026");
    expect(pushedParams().get("through_at")).toBe("2026-10-31T23:59:59.999Z");
    expect(pushedParams().get("fleet")).toBe("deleted-fleet");
    expect(pushedParams().has("c")).toBe(false);
    current.params = pushedParams();
    rendered.unmount();
    view(billingReport({from_at: "2026-10-01T00:00:00Z", through_at: "2026-10-31T23:59:59.999Z"}));
    await choose("Period", "November 2026");
    expect(pushedParams().has("through_at")).toBe(false);
    expect(pushedParams().get("fleet")).toBe("deleted-fleet");
    expect(screen.getByText(/Billing history includes deleted fleets/)).toBeTruthy();
  });
  it("renders full-period totals with the recorded physical runtime count", () => {
    view(billingReport({ totals: billingTotals({ charged_nanos: "5200000000", activity_count: 52, line_count: 150 }) }));
    const totals = within(screen.getByLabelText("Period totals"));
    expect(totals.getByText("$5.20")).toBeTruthy();
    expect(totals.getByText("52")).toBeTruthy();
    expect(totals.getByText("Tokens")).toBeTruthy();
    expect(totals.getByText("50")).toBeTruthy();
    expect(screen.queryByRole("button", { name: /Payment alerts/ })).toBeNull();
    expect(screen.queryByRole("button", {name: "Charge details"})).toBeNull();
    expect(screen.queryByRole("button", {name: "More filters"})).toBeNull();
    expect(screen.getByText(/Totals exclude pending or unavailable provider usage/)).toBeTruthy();
  });

  it("retains an older selected calendar month outside the recent choices", async () => {
    current.params = new URLSearchParams("through_at=2024-02-29T23:59:59.999Z");
    view(billingReport({from_at: "2024-02-01T00:00:00Z", through_at: "2024-02-29T23:59:59.999Z"}));
    expect(screen.getByRole("combobox", {name: "Period"}).textContent).toBe("February 2024");
    await choose("Period", "November 2026");
    expect(pushedParams().has("through_at")).toBe(false);
  });

  it.each<[string, string, string, string | null]>([["2026-11-01T00:00:00Z", "2026-11-05T12:00:00Z", "November 2026", null],
    ["2026-10-01T00:00:00Z", "2026-10-05T12:00:00Z", "October 2026", "2026-10-31T23:59:59.999Z"]])(
    "can replace a partial-month snapshot with its canonical month (%s)", async (from_at, through_at, label, cutoff) => {
      current.params = new URLSearchParams({through_at, fleet: BILLING_FLEET, c: "old"});
      view(billingReport({from_at, through_at}));
      expect(screen.getByRole("combobox", {name: "Period"}).textContent).toContain("through");
      await choose("Period", label);
      expect(pushedParams().get("through_at")).toBe(cutoff);
      expect(pushedParams().get("fleet")).toBe(BILLING_FLEET);
      expect(pushedParams().has("c")).toBe(false);
    });

  it("labels an explicitly scoped report without silently widening it", () => {
    current.params = new URLSearchParams("fleet=setup&period=today");
    view(billingReport({period: "today"}));
    expect(screen.getByRole("combobox", {name: "Fleet"}).textContent).toBe("Setup chat");
    expect(screen.getByRole("combobox", {name: "Period"}).textContent).toBe("Today");
    expect(push).not.toHaveBeenCalled();
  });

  it.each([["Period", "Last 7 days", "period", "week"], ["Fleet", "Payment alerts", "fleet", BILLING_FLEET]])(
    "changing %s resets paging while retaining the selected period", async (label, choice, key, expected) => {
      current.params = new URLSearchParams("c=old&cps=25&through_at=old&activity=old&dc=old");
      view();
      await choose(label, choice);
      const params = pushedParams();
      expect(params.get(key)).toBe(expected);
      for (const stale of ["c", "cps", "activity", "dc"]) expect(params.has(stale)).toBe(false);
      expect(params.get("through_at")).toBe(label === "Period" ? null : "old");
    },
  );

  it("filter choices keep a deleted fleet selectable without duplicating private setup", async () => {
    const retained = "01924f4e-0000-7000-8000-00000000c4a9";
    view(billingReport({ filters: { fleets: [{ id: null, name: null }, { id: retained, name: null }], models: [] } }));
    const trigger = screen.getByRole("combobox", { name: "Fleet" });
    fireEvent.click(trigger);
    fireEvent.keyDown(trigger, { key: "Enter" });
    expect(screen.queryByRole("option", { name: "Setup chat" })).toBeNull();
    expect(screen.getAllByRole("option")[0]?.textContent).toBe("All Fleets");
    fireEvent.click(screen.getByRole("option", { name: retained }));
    await waitFor(() => expect(pushedParams().get("fleet")).toBe(retained));
  });

  it("keeps a successful empty report distinct from recovery", () => {
    view(billingReport({ totals: billingTotals({charged_nanos: "0", activity_count: 0, line_count: 0}),
      daily: [], spending: {by_fleet: [], by_model: []}, filters: {fleets: [], models: []} }));
    expect(screen.getByText("No settled charges for this period.")).toBeTruthy();
    expect(screen.queryByText("Billing report unavailable")).toBeNull();
    expect(screen.queryByRole("button", {name: "Next page"})).toBeNull();
    expect(screen.queryByRole("button", {name: "More filters"})).toBeNull();
    expect(screen.queryByRole("combobox", {name: "Model"})).toBeNull();
  });

  it("keeps an empty scoped fleet labelled and can clear it without changing the period", async () => {
    current.params = new URLSearchParams({fleet: BILLING_FLEET, period: "month", through_at: BILLING_AT});
    view(billingReport({totals: billingTotals({charged_nanos: "0", activity_count: 0, line_count: 0}),
      daily: [], spending: {by_fleet: [], by_model: []}, filters: {fleets: [], models: []}}));
    expect(screen.getByRole("combobox", {name: "Fleet"}).textContent).toBe(BILLING_FLEET);
    expect(screen.getByText(`Daily spend · ${BILLING_FLEET}`)).toBeTruthy();
    expect(screen.getByText("No settled charges for this period.")).toBeTruthy();
    expect(push).not.toHaveBeenCalled();
    await choose("Fleet", "All Fleets");
    expect(Object.fromEntries(pushedParams())).toEqual({period: "month", through_at: BILLING_AT});
  });

  it("treats an empty fleet query as All Fleets, matching the report request", () => {
    current.params = new URLSearchParams("fleet=");
    view();
    expect(screen.getByRole("combobox", {name: "Fleet"}).textContent).toBe("All Fleets");
    expect(screen.getByText("Daily spend · All Fleets")).toBeTruthy();
    expect(push).not.toHaveBeenCalled();
  });
});
