import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

vi.mock("lucide-react", () => ({
  ActivityIcon: () => React.createElement("svg", { "data-icon": "ActivityIcon" }),
  Loader2Icon: () => React.createElement("svg", { "data-icon": "Loader2Icon" }),
  ArrowUp: () => React.createElement("svg", { "data-icon": "ArrowUp" }),
  ArrowDown: () => React.createElement("svg", { "data-icon": "ArrowDown" }),
  ArrowUpDown: () => React.createElement("svg", { "data-icon": "ArrowUpDown" }),
}));

// Paging is URL state: the pager navigates and the Server Component fetches
// the page named by the cursor, so there is no client-side fetch to mock.
const { routerPushMock, searchParamsRef } = vi.hoisted(() => ({
  routerPushMock: vi.fn(),
  searchParamsRef: { current: new URLSearchParams() },
}));

vi.mock("next/navigation", () => ({
  useRouter: () => ({ push: routerPushMock }),
  usePathname: () => "/settings/billing",
  useSearchParams: () => searchParamsRef.current,
}));

import BillingUsageTab from "@/app/(dashboard)/settings/billing/components/BillingUsageTab";
import { chargeAgentLabel, type ChargeRow } from "@/app/(dashboard)/settings/billing/lib/charges";
import type { ActionResult } from "@/lib/actions/with-token";
import { CHARGE_TYPE, PROVIDER_MODE, type TenantBillingChargesResponse } from "@/lib/types";

// $0.001 — a representative sub-cent stage charge; the "−$0.001" row assertions
// below depend on this exact value.
const SAMPLE_CHARGE_NANOS = 1_000_000;

function charge(over: Partial<ChargeRow> = {}): ChargeRow {
  return {
    id: "tel_1",
    tenant_id: "t_1",
    workspace_id: "w_1",
    fleet_id: "z_1",
    event_id: "evt_1",
    charge_type: CHARGE_TYPE.stage,
    posture: PROVIDER_MODE.platform,
    model: "kimi-k2.6",
    credit_deducted_nanos: SAMPLE_CHARGE_NANOS,
    token_count_input: 820,
    token_count_output: 1040,
    wall_ms: 3000,
    recorded_at: 1_700_000_000_000,
    ...over,
  };
}

beforeEach(() => {
  routerPushMock.mockReset();
  searchParamsRef.current = new URLSearchParams();
});
afterEach(() => cleanup());

describe("BillingUsageTab (test_billing_usage_ledger_and_empty)", () => {
  it("renders the empty-state when there are no charges and no cursor", () => {
    render(React.createElement(BillingUsageTab, { initialCharges: [], initialCursor: null }));
    expect(screen.getByText("No charges yet")).toBeTruthy();
    expect(screen.queryByTestId("pagination-page")).toBeNull();
  });

  it("offers a way forward when an empty first page carries a cursor", () => {
    render(React.createElement(BillingUsageTab, { initialCharges: [], initialCursor: "tok_page2" }));
    expect(screen.getByText("No charges yet")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Next page" })).toBeTruthy();
  });

  it("renders a ledger row with date, fleet identity, activity, and amount", () => {
    render(React.createElement(BillingUsageTab, { initialCharges: [charge()], initialCursor: null }));
    // amount is a deduction (negative)
    expect(screen.getByText("−$0.001")).toBeTruthy();
    // fleet identity and model make the debit attributable
    expect(screen.getByText(chargeAgentLabel(charge()))).toBeTruthy();
    expect(screen.getByText("kimi k2.6")).toBeTruthy();
    // activity makes the token counts legible
    expect(screen.getByText("Run · 820 input tokens · 1,040 output tokens")).toBeTruthy();
    // date column renders the formatted timestamp
    expect(screen.getByText(/\d{4} · \d{2}:\d{2}/)).toBeTruthy();
  });

  it("describes a receive charge as an event receipt", () => {
    render(
      React.createElement(BillingUsageTab, {
        initialCharges: [charge({ charge_type: CHARGE_TYPE.receive, token_count_input: null, token_count_output: null })],
        initialCursor: null,
      }),
    );
    expect(screen.getByText("Event received")).toBeTruthy();
  });

  it("renders a zero debit without a negative sign", () => {
    render(React.createElement(BillingUsageTab, {
      initialCharges: [charge({ credit_deducted_nanos: 0 })],
      initialCursor: null,
    }));
    expect(screen.getByText("$0.00")).toBeTruthy();
    expect(screen.queryByText("−$0.00")).toBeNull();
  });

  it("wires DataTable's stickyHeader on the usage ledger (its real consumer)", () => {
    render(React.createElement(BillingUsageTab, { initialCharges: [charge()], initialCursor: null }));
    const region = screen.getByRole("region", { name: /usage history, scrollable/i });
    expect(region.querySelector("table")).toBeTruthy();
    expect(region.getAttribute("tabindex")).toBe("0");
  });

  it("sorts every usage data column from its header arrow", () => {
    render(React.createElement(BillingUsageTab, { initialCharges: [
      charge(),
      charge({ id: "tel_2", recorded_at: 1_800_000_000_000, credit_deducted_nanos: 5_000_000 }),
    ], initialCursor: null }));

    for (const name of ["Date", "Fleet and model", "Activity", "Amount"]) {
      fireEvent.click(screen.getByRole("button", { name }));
      expect(screen.getByRole("columnheader", { name }).getAttribute("aria-sort")).not.toBe("none");
      if (name === "Amount") expect(screen.getAllByRole("row")[1]?.textContent).toContain("−$0.005");
    }
  });

  it("shows no pager when the ledger fits one page", () => {
    render(React.createElement(BillingUsageTab, { initialCharges: [charge()], initialCursor: null }));
    expect(screen.queryByTestId("pagination-page")).toBeNull();
  });

  it("puts the page turn in the URL so a reload lands on the same page", async () => {
    render(React.createElement(BillingUsageTab, { initialCharges: [charge()], initialCursor: "tok_page2" }));
    fireEvent.click(screen.getByRole("button", { name: "Next page" }));
    await waitFor(() => expect(routerPushMock).toHaveBeenCalled());
    expect(String(routerPushMock.mock.calls[0]?.[0])).toContain("c=tok_page2");
  });

  it("walks back down the trail to the first page", async () => {
    searchParamsRef.current = new URLSearchParams("c=tok_page2");
    render(React.createElement(BillingUsageTab, { initialCharges: [charge()], initialCursor: null }));
    expect(screen.getByText("Page 2")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Previous page" }));
    await waitFor(() => expect(routerPushMock).toHaveBeenCalled());
    // Page one carries no cursor, so the trail empties rather than shrinking.
    expect(String(routerPushMock.mock.calls[0]?.[0])).not.toContain("c=");
  });
});
