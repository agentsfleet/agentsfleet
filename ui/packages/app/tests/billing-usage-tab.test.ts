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

const { listChargesActionMock } = vi.hoisted(() => ({
  listChargesActionMock: vi.fn(),
}));

vi.mock("@/app/(dashboard)/settings/billing/actions", () => ({
  listTenantBillingChargesAction: listChargesActionMock,
}));

import BillingUsageTab from "@/app/(dashboard)/settings/billing/components/BillingUsageTab";
import type { ChargeRow } from "@/app/(dashboard)/settings/billing/lib/charges";
import { CHARGE_TYPE, PROVIDER_MODE } from "@/lib/types";

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
  listChargesActionMock.mockReset();
});
afterEach(() => cleanup());

describe("BillingUsageTab (test_billing_usage_ledger_and_empty)", () => {
  it("renders the empty-state when there are no charges and no cursor", () => {
    render(React.createElement(BillingUsageTab, { initialCharges: [], initialCursor: null }));
    expect(screen.getByText("No charges yet")).toBeTruthy();
    expect(screen.queryByTestId("pagination-cursor")).toBeNull();
  });

  it("renders a ledger row with date · amount · type · description", () => {
    render(React.createElement(BillingUsageTab, { initialCharges: [charge()], initialCursor: null }));
    // amount is a deduction (negative)
    expect(screen.getByText("−$0.001")).toBeTruthy();
    // type = posture badge
    expect(screen.getByText(PROVIDER_MODE.platform)).toBeTruthy();
    // description carries the model + token detail
    expect(screen.getByText("kimi-k2.6 · run · 820→1040 tok")).toBeTruthy();
    // date column renders the formatted timestamp
    expect(screen.getByText(/\d{4} · \d{2}:\d{2}/)).toBeTruthy();
  });

  it("describes a receive charge as a gate-pass", () => {
    render(
      React.createElement(BillingUsageTab, {
        initialCharges: [charge({ charge_type: CHARGE_TYPE.receive, token_count_input: null, token_count_output: null })],
        initialCursor: null,
      }),
    );
    expect(screen.getByText("kimi-k2.6 · event gate-pass")).toBeTruthy();
  });

  it("uses the cyan badge variant for self_managed posture", () => {
    const { container } = render(
      React.createElement(BillingUsageTab, {
        initialCharges: [charge({ posture: PROVIDER_MODE.self_managed })],
        initialCursor: null,
      }),
    );
    expect(container.textContent).toContain(PROVIDER_MODE.self_managed);
  });

  it("wires DataTable's stickyHeader on the usage ledger (its real consumer)", () => {
    render(React.createElement(BillingUsageTab, { initialCharges: [charge()], initialCursor: null }));
    const region = screen.getByRole("region", { name: /usage history, scrollable/i });
    expect(region.querySelector("table")).toBeTruthy();
    expect(region.getAttribute("tabindex")).toBe("0");
  });

  it("sorts every usage data column from its header arrow", () => {
    render(React.createElement(BillingUsageTab, { initialCharges: [charge(), charge({ id: "tel_2", recorded_at: 1_800_000_000_000 })], initialCursor: null }));

    for (const name of ["Date", "Amount", "Type", "Description"]) {
      fireEvent.click(screen.getByRole("button", { name }));
      expect(screen.getByRole("columnheader", { name }).getAttribute("aria-sort")).not.toBe("none");
    }
  });

  it("hides Load more when there is no cursor", () => {
    render(React.createElement(BillingUsageTab, { initialCharges: [charge()], initialCursor: null }));
    expect(screen.queryByTestId("pagination-cursor")).toBeNull();
  });

  it("fetches and appends the next page on Load more click", async () => {
    listChargesActionMock.mockResolvedValue({
      ok: true,
      data: {
        items: [charge({ id: "tel_2", event_id: "evt_2", recorded_at: 1_700_000_500_000 })],
        next_cursor: null,
      },
    });
    render(React.createElement(BillingUsageTab, { initialCharges: [charge()], initialCursor: "tok_page2" }));
    fireEvent.click(screen.getByRole("button", { name: "Load more items" }));
    await waitFor(() => expect(screen.getAllByText(/kimi-k2\.6/).length).toBe(2));
    expect(listChargesActionMock).toHaveBeenCalledWith({ limit: 50, cursor: "tok_page2" });
    await waitFor(() => expect(screen.queryByTestId("pagination-cursor")).toBeNull());
  });

  it("de-dupes by charge id when a page boundary repeats a row", async () => {
    listChargesActionMock.mockResolvedValue({
      ok: true,
      data: { items: [charge({ id: "tel_1" })], next_cursor: null }, // same id as initial
    });
    render(React.createElement(BillingUsageTab, { initialCharges: [charge({ id: "tel_1" })], initialCursor: "tok" }));
    fireEvent.click(screen.getByRole("button", { name: "Load more items" }));
    await waitFor(() => expect(screen.queryByTestId("pagination-cursor")).toBeNull());
    expect(screen.getAllByText("−$0.001").length).toBe(1);
  });

  it("surfaces a 'Not authenticated' alert when the action returns unauthenticated", async () => {
    listChargesActionMock.mockResolvedValue({ ok: false, error: "Not authenticated", status: 401 });
    render(React.createElement(BillingUsageTab, { initialCharges: [charge()], initialCursor: "tok" }));
    fireEvent.click(screen.getByRole("button", { name: "Load more items" }));
    await waitFor(() => expect(screen.getByRole("alert").textContent).toContain("Not authenticated"));
  });

  it("surfaces a fetch error inline without losing the previous page", async () => {
    listChargesActionMock.mockResolvedValue({ ok: false, error: "503 service unavailable" });
    render(React.createElement(BillingUsageTab, { initialCharges: [charge()], initialCursor: "tok" }));
    fireEvent.click(screen.getByRole("button", { name: "Load more items" }));
    await waitFor(() => expect(screen.getByRole("alert").textContent).toContain("503 service unavailable"));
    expect(screen.getByText("kimi-k2.6 · run · 820→1040 tok")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Load more items" })).toBeTruthy();
  });
});
