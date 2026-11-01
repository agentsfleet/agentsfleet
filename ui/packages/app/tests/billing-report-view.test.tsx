import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { TooltipProvider } from "@agentsfleet/design-system";
import { BillingReportView } from "@/app/(dashboard)/settings/billing/components/BillingReportView";
import { BillingActivities } from "@/app/(dashboard)/settings/billing/components/BillingActivities";
import { BillingActivityDetails } from "@/app/(dashboard)/settings/billing/components/BillingActivityDetails";
import { BillingSpending } from "@/app/(dashboard)/settings/billing/components/BillingSpending";
import type { BillingDetail } from "@/app/(dashboard)/settings/billing/lib/report-detail";
import { BILLING_DETAIL_STATE } from "@/app/(dashboard)/settings/billing/lib/report-detail";
import { activityKey } from "@/app/(dashboard)/settings/billing/lib/report-query";
import type { BillingReport } from "@/lib/api/billing_report";
import { BILLING_ACTIVITY_SETUP } from "@/lib/api/billing_report";
import { PROVIDER_MODE } from "@/lib/types";
import { BILLING_AT, BILLING_MODEL, BILLING_MODEL_NAME, BILLING_SELECTION, billingActivity, billingCharge, billingReport, billingTotals } from "./helpers/billing-report";

const { push, refresh, current } = vi.hoisted(() => ({ push: vi.fn(), refresh: vi.fn(), current: { params: new URLSearchParams() } }));
vi.mock("next/navigation", () => ({
  useRouter: () => ({ push, refresh }), usePathname: () => "/settings/billing", useSearchParams: () => current.params,
}));

const names = { [BILLING_MODEL]: BILLING_MODEL_NAME };
const SPENDING_TAB = "Spending breakdown";
const BY_FLEET_LABEL = "Charged by fleet or setup chat";
const BY_MODEL_LABEL = "Charged by model";
const closed: BillingDetail = { status: BILLING_DETAIL_STATE.closed };
const unavailable: BillingDetail = { status: BILLING_DETAIL_STATE.unavailable };
function ready(): BillingDetail {
  return { status: BILLING_DETAIL_STATE.ready, page: 1, charges: { items: [billingCharge()], total: null, next_cursor: null } };
}
function view(report = billingReport(), detail = closed, selected = "") {
  return render(<BillingReportView report={report} pageSize={25} modelNames={names} detail={detail} selected={selected} />, { wrapper: TooltipProvider });
}
function activities(report: BillingReport, detail = closed, selected = "") {
  return render(<BillingActivities report={report} pageSize={25} modelNames={names} detail={detail} selected={selected} />, { wrapper: TooltipProvider });
}
function details(detail: BillingDetail, modelNames: Record<string, string> = names, activity = billingActivity()) {
  return render(<BillingActivityDetails activity={activity} detail={detail} modelNames={modelNames} />, { wrapper: TooltipProvider });
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

beforeEach(() => { push.mockReset(); refresh.mockReset(); current.params = new URLSearchParams(); });
afterEach(() => cleanup());

describe("Billing report filters and details preserve period totals", () => {
  it("opens the previous UTC month and returns to the current month without losing fleet selection", () => {
    current.params = new URLSearchParams("fleet=deleted-fleet&through_at=2026-11-01T01:00:00Z&c=old&activity=old");
    view(billingReport({from_at: "2026-11-01T00:00:00Z", through_at: "2026-11-01T01:00:00Z"}));
    fireEvent.click(screen.getByRole("button", {name: "Previous month"}));
    expect(pushedParams().get("through_at")).toBe("2026-10-31T23:59:59.999Z");
    expect(pushedParams().get("fleet")).toBe("deleted-fleet");
    expect(pushedParams().has("c")).toBe(false);
    fireEvent.click(screen.getByRole("button", {name: "Current month"}));
    expect(pushedParams().has("through_at")).toBe(false);
    expect(pushedParams().get("fleet")).toBe("deleted-fleet");
    expect(screen.getByText(/Billing history includes deleted fleets/)).toBeTruthy();
  });
  it("renders full-period totals independently of the single displayed activity", () => {
    view(billingReport({ totals: billingTotals({ charged_nanos: "5200000000", activity_count: 52, line_count: 150 }) }));
    expect(screen.getByText("$5.20")).toBeTruthy();
    expect(screen.getByText("52")).toBeTruthy();
    expect(screen.getByText("Input / output tokens")).toBeTruthy();
    expect(screen.getByText("40")).toBeTruthy();
    expect(screen.getAllByRole("button", { name: /Payment alerts/ })).toHaveLength(1);
    expect(screen.getByText(/Totals exclude pending or unavailable provider usage/)).toBeTruthy();
  });

  it("an empty successful report has an honest empty state and a disabled next page", () => {
    view(billingReport({ totals: billingTotals({ charged_nanos: "0", activity_count: 0, line_count: 0 }),
      activities: { items: [], total: 0, next_cursor: null }, spending: { by_fleet: [], by_model: [] }, filters: { fleets: [], models: [] } }));
    expect(screen.getByText("No settled activity")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Next page" }).hasAttribute("disabled")).toBe(true);
    fireEvent.mouseDown(screen.getByRole("tab", { name: SPENDING_TAB }));
    fireEvent.click(screen.getByRole("tab", { name: SPENDING_TAB }));
    expect(screen.getAllByText("No settled spending")).toHaveLength(1);
  });

  it.each([["Period", "Today", "period", "today"], ["Fleet", "Setup chat", "fleet", "setup"], ["Model", BILLING_MODEL_NAME, "model", BILLING_MODEL]])(
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

  it("Enter opens an activity's details at the captured report cutoff", async () => {
    const user = userEvent.setup();
    activities(billingReport());
    screen.getByRole("button", { name: /Payment alerts/ }).focus();
    await user.keyboard("{Enter}");
    await waitFor(() => expect(push).toHaveBeenCalled());
    expect(pushedParams().get("activity")).toBe(BILLING_SELECTION);
    expect(pushedParams().get("through_at")).toBe(BILLING_AT);
  });

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

  it("Space closes an activity and clears its detail continuation while keeping the report", async () => {
    const user = userEvent.setup();
    current.params = new URLSearchParams(`activity=${BILLING_SELECTION}&dc=old`);
    activities(billingReport(), ready(), BILLING_SELECTION);
    screen.getByRole("button", { name: /Payment alerts/ }).focus();
    await user.keyboard(" ");
    await waitFor(() => expect(push).toHaveBeenCalled());
    expect(pushedParams().has("activity")).toBe(false);
    expect(pushedParams().has("dc")).toBe(false);
    expect(pushedParams().get("through_at")).toBe(BILLING_AT);
  });

  it("main-page next and previous retain the report cutoff", async () => {
    const report = billingReport({ activities: { items: [billingActivity()], total: 52, next_cursor: "second" } });
    const rendered = activities(report);
    fireEvent.click(screen.getByRole("button", { name: "Next page" }));
    await waitFor(() => expect(push).toHaveBeenCalled());
    expect(pushedParams().get("through_at")).toBe(BILLING_AT);
    expect(pushedParams().getAll("c")).toEqual(["second"]);
    current.params = pushedParams();
    rendered.unmount();
    activities(report);
    fireEvent.click(screen.getByRole("button", { name: "Previous page" }));
    await waitFor(() => expect(push).toHaveBeenCalledTimes(2));
    expect(pushedParams().has("c")).toBe(false);
    expect(pushedParams().get("through_at")).toBe(BILLING_AT);
  });

  it("an unknown activity count retains cursor navigation without inventing a total", async () => {
    activities(billingReport({ activities: { items: [billingActivity()], total: null, next_cursor: "next-activity" } }));
    expect(screen.getByText("Page 1")).toBeTruthy();
    expect(screen.queryByText(/of .* activities/)).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Next page" }));
    await waitFor(() => expect(pushedParams().getAll("c")).toEqual(["next-activity"]));
    expect(pushedParams().get("through_at")).toBe(BILLING_AT);
  });

  it("changing page size clears the main trail but retains the report cutoff", async () => {
    current.params = new URLSearchParams("c=old&cps=25");
    activities(billingReport());
    await choose("Rows per page", "50");
    expect(pushedParams().get("ps")).toBe("50");
    expect(pushedParams().has("c")).toBe(false);
    expect(pushedParams().get("through_at")).toBe(BILLING_AT);
  });

  it("shows setup attribution, mixed provider billing and exact raw models without a catalogue", () => {
    const setup = billingActivity({ activity_type: BILLING_ACTIVITY_SETUP, fleet_id: null, fleet_name: null,
      postures: [PROVIDER_MODE.platform, PROVIDER_MODE.self_managed], models: ["unknown-model"] });
    render(<BillingActivities report={billingReport({ activities: { items: [setup], total: 1, next_cursor: null } })}
      pageSize={25} modelNames={{}} selected={activityKey(setup)} detail={ready()} />, { wrapper: TooltipProvider });
    expect(screen.getByRole("button", { name: /Setup chat/ })).toBeTruthy();
    expect(screen.getAllByText("unknown-model")).toHaveLength(2);
    expect(screen.getByText(/billed by your provider/)).toBeTruthy();
    expect(screen.getByText("40 / 10")).toBeTruthy();
  });

  it("the spending view uses complete server groups and recorded names", () => {
    view();
    fireEvent.mouseDown(screen.getByRole("tab", { name: SPENDING_TAB }));
    fireEvent.click(screen.getByRole("tab", { name: SPENDING_TAB }));
    expect(within(screen.getByRole("list", { name: BY_FLEET_LABEL })).getByText("Payment alerts")).toBeTruthy();
    expect(within(screen.getByRole("list", { name: BY_MODEL_LABEL })).getByText(BILLING_MODEL_NAME)).toBeTruthy();
  });

  it("spending groups retain opaque labels and private setup when names are absent", () => {
    const totals = billingTotals();
    render(<BillingSpending modelNames={{}} report={billingReport({ spending: {
      by_fleet: [{ key: null, label: null, totals }, { key: "retained-fleet", label: null, totals }],
      by_model: [{ key: BILLING_MODEL, label: null, totals }, { key: null, label: null, totals }],
    } })} />, { wrapper: TooltipProvider });
    expect(screen.getByText("Setup chat")).toBeTruthy();
    expect(screen.getByText("retained-fleet")).toBeTruthy();
    expect(screen.getByText(BILLING_MODEL)).toBeTruthy();
    expect(screen.getByText("Unknown model")).toBeTruthy();
  });

  it("a zero-charge group keeps a zero bar and the other breakdown's empty state", () => {
    render(<BillingSpending modelNames={names} report={billingReport({ spending: {
      by_fleet: [{ key: null, label: null, totals: billingTotals({ charged_nanos: "0" }) }], by_model: [],
    } })} />);
    const group = screen.getByRole("list", { name: BY_FLEET_LABEL });
    expect(within(group).getByText("$0.00")).toBeTruthy();
    expect(group.querySelector<HTMLElement>(".usage-bar-fill")?.style.width).toBe("0%");
    expect(screen.getByText("No settled spending")).toBeTruthy();
  });

  it("preserves exact money and proportional bars above safe JavaScript integers", () => {
    render(<BillingSpending modelNames={names} report={billingReport({ spending: {
      by_fleet: [
        { key: "large", label: "Large charge", totals: billingTotals({ charged_nanos: "10000000000000000100000" }) },
        { key: "small", label: "Smaller charge", totals: billingTotals({ charged_nanos: "2500000000000000025000" }) },
      ], by_model: [],
    } })} />);
    const group = screen.getByRole("list", { name: BY_FLEET_LABEL });
    expect(within(group).getByText("$10,000,000,000,000.0001")).toBeTruthy();
    const bars = group.querySelectorAll<HTMLElement>(".usage-bar-fill");
    expect(Array.from(bars, bar => bar.style.width)).toEqual(["100%", "25%"]);
  });
});

describe("individual activity charges", () => {
  it("does not render or fetch a closed disclosure", () => {
    const { container } = details(closed);
    expect(container.textContent).toBe("");
  });

  it("a failed read retains a usable retry action", async () => {
    details(unavailable);
    expect(screen.getByText("Charges unavailable")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Retry charges" }));
    await waitFor(() => expect(refresh).toHaveBeenCalledOnce());
  });

  it("shows exact tiny debits and explicitly priced and unpriced recorded quantities", () => {
    details({ status: BILLING_DETAIL_STATE.ready, page: 1, charges: { items: [billingCharge({ charged_nanos: "1", unpriced_tokens: 5, posture: PROVIDER_MODE.self_managed })], total: null, next_cursor: null } }, {},
      billingActivity({ totals: billingTotals({ unpriced_tokens: 5 }), postures: [PROVIDER_MODE.self_managed] }));
    expect(screen.getByText("$0.000000001")).toBeTruthy();
    expect(screen.getByText(BILLING_MODEL)).toBeTruthy();
    expect(screen.getByText(/Own provider key:/)).toBeTruthy();
    expect(screen.getByText(/5 tokens awaiting/)).toBeTruthy();
    expect(screen.getByText(/Separate historical model and runtime prices are unavailable/)).toBeTruthy();
  });

  it("an empty detail page stays distinct from a failed read", () => {
    details({ status: BILLING_DETAIL_STATE.ready, page: 1, charges: { items: [], total: null, next_cursor: null } });
    expect(screen.getByText("No matching settled charges")).toBeTruthy();
    expect(screen.queryByText("Charges unavailable")).toBeNull();
  });

  it("aligns recorded inference and runtime costs without inventing a price split", () => {
    const activity = billingActivity({ totals: billingTotals({ charged_nanos: "169000", input_tokens: 12, output_tokens: 3, runtime_ms: 250 }) });
    details({ status: BILLING_DETAIL_STATE.ready, page: 1, charges: { items: [
      billingCharge({ id: "inference", charged_nanos: "109000", input_tokens: 12, output_tokens: 3, runtime_ms: 0 }),
      billingCharge({ id: "runtime", charged_nanos: "60000", input_tokens: 0, output_tokens: 0, runtime_ms: 250 }),
    ], total: 2, next_cursor: null } }, names, activity);
    const lines = screen.getByLabelText("Activity charge details");
    expect(within(lines).getByText("12 / 3")).toBeTruthy();
    expect(screen.getByText("Inference").nextElementSibling?.textContent).toBe("$0.000109");
    expect(screen.getByText("Runtime · 0.25 seconds").nextElementSibling?.textContent).toBe("$0.00006");
    expect(screen.getByText("Total charged").nextElementSibling?.textContent).toBe("$0.000169");
    expect(screen.queryByText(/Separate historical/)).toBeNull();
    expect(screen.queryByRole("navigation", { name: "Charge pagination" })).toBeNull();
    expect(screen.queryByRole("table")).toBeNull();
    expect(screen.queryByText("Model usage and runtime charged to credits.")).toBeNull();
  });

  it("retains model attribution and a combined charge instead of guessing component costs", () => {
    const other = "another-model";
    details({ status: BILLING_DETAIL_STATE.ready, page: 1, charges: {
      items: [billingCharge()], total: 1, next_cursor: null,
    } }, names, billingActivity({ models: [BILLING_MODEL, other] }));
    expect(screen.getByText(`Inference & runtime · 1 second · ${BILLING_MODEL_NAME}`).nextElementSibling?.textContent).toBe("$0.0001");
    expect(screen.getByText(`${BILLING_MODEL_NAME}, ${other}`)).toBeTruthy();
    expect(screen.getByText(/Separate historical model and runtime prices are unavailable/)).toBeTruthy();
    expect(screen.queryByText("Inference")).toBeNull();
    expect(screen.queryByText("Runtime")).toBeNull();
  });

  it("retains raw model attribution and a recorded debit when quantity metadata is absent", () => {
    const rawModel = "unlisted-recorded-model";
    details({ status: BILLING_DETAIL_STATE.ready, page: 1, charges: { items: [billingCharge({
      model: rawModel, charged_nanos: "1", input_tokens: 0, output_tokens: 0, runtime_ms: 0,
    })], total: 1, next_cursor: null } }, names, billingActivity({ models: [BILLING_MODEL, rawModel] }));
    expect(screen.getByText(`Recorded charge · ${rawModel}`).nextElementSibling?.textContent).toBe("$0.000000001");
    expect(screen.getByText(`${BILLING_MODEL_NAME}, ${rawModel}`)).toBeTruthy();
    expect(screen.queryByText(/Separate historical/)).toBeNull();
  });

  it("next and previous detail pages retain their activity and report scope", async () => {
    current.params = new URLSearchParams(`activity=${BILLING_SELECTION}&through_at=${BILLING_AT}`);
    const rendered = details({ status: BILLING_DETAIL_STATE.ready, page: 1, charges: { items: [billingCharge()], total: null, next_cursor: "second" } });
    fireEvent.click(screen.getByRole("button", { name: "Next page" }));
    await waitFor(() => expect(push).toHaveBeenCalledOnce());
    expect(pushedParams().getAll("dc")).toEqual(["second"]);
    expect(pushedParams().get("activity")).toBe(BILLING_SELECTION);
    expect(screen.getByText("Showing charge page 1. Total charged covers the entire activity.")).toBeTruthy();
    current.params = pushedParams();
    rendered.unmount();
    details({ status: BILLING_DETAIL_STATE.ready, page: 2, charges: { items: [billingCharge()], total: null, next_cursor: null } });
    fireEvent.click(screen.getByRole("button", { name: "Previous page" }));
    await waitFor(() => expect(push).toHaveBeenCalledTimes(2));
    expect(pushedParams().has("dc")).toBe(false);
    expect(pushedParams().get("through_at")).toBe(BILLING_AT);
  });

  it("a stale detail page cannot navigate without its continuation trail", () => {
    current.params = new URLSearchParams(`activity=${BILLING_SELECTION}&through_at=${BILLING_AT}`);
    details({ status: BILLING_DETAIL_STATE.ready, page: 2, charges: { items: [billingCharge()], total: null, next_cursor: null } });
    fireEvent.click(screen.getByRole("button", { name: "Previous page" }));
    expect(push).not.toHaveBeenCalled();
    expect(screen.getByLabelText("Activity charge details")).toBeTruthy();
    expect(current.params.get("activity")).toBe(BILLING_SELECTION);
    expect(current.params.get("through_at")).toBe(BILLING_AT);
  });
});
