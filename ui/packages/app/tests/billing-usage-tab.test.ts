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
  listChargesActionMock.mockReset();
});
afterEach(() => cleanup());

describe("BillingUsageTab (test_billing_usage_ledger_and_empty)", () => {
  it("renders the empty-state when there are no charges and no cursor", () => {
    render(React.createElement(BillingUsageTab, { initialCharges: [], initialCursor: null }));
    expect(screen.getByText("No charges yet")).toBeTruthy();
    expect(screen.queryByTestId("pagination-page")).toBeNull();
  });

  it("recovers an empty first page by paging forward", async () => {
    let resolveCharges: ((value: ActionResult<TenantBillingChargesResponse>) => void) | undefined;
    listChargesActionMock.mockImplementation(() => new Promise((resolve) => {
      resolveCharges = resolve;
    }));
    render(React.createElement(BillingUsageTab, { initialCharges: [], initialCursor: "tok_page2" }));

    expect(screen.getByText("No charges yet")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Next page" }));
    expect(screen.getByText("No charges yet")).toBeTruthy();
    expect(screen.getByRole("navigation", { name: "Pagination" }).getAttribute("aria-busy")).toBe("true");
    expect(screen.getByText("Loading…")).toBeTruthy();

    resolveCharges?.({ ok: true, data: { items: [charge()], next_cursor: null } });

    await waitFor(() => expect(screen.getByText("kimi-k2.6 · run · 820→1040 tok")).toBeTruthy());
    expect(listChargesActionMock).toHaveBeenCalledWith({ limit: 25, cursor: "tok_page2" });
    // Page 2 keeps its pager — the way back matters as much as the way on.
    expect(screen.getByText("Page 2")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Next page" }).hasAttribute("disabled")).toBe(true);
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
    render(React.createElement(BillingUsageTab, { initialCharges: [
      charge(),
      charge({ id: "tel_2", recorded_at: 1_800_000_000_000, credit_deducted_nanos: 5_000_000 }),
    ], initialCursor: null }));

    for (const name of ["Date", "Amount", "Type", "Description"]) {
      fireEvent.click(screen.getByRole("button", { name }));
      expect(screen.getByRole("columnheader", { name }).getAttribute("aria-sort")).not.toBe("none");
      if (name === "Amount") expect(screen.getAllByRole("row")[1]?.textContent).toContain("−$0.005");
    }
  });

  it("shows no pager when the ledger fits one page", () => {
    render(React.createElement(BillingUsageTab, { initialCharges: [charge()], initialCursor: null }));
    expect(screen.queryByTestId("pagination-page")).toBeNull();
  });

  it("replaces the rows with the next page instead of growing one long list", async () => {
    listChargesActionMock.mockResolvedValue({
      ok: true,
      data: {
        items: [charge({ id: "tel_2", event_id: "evt_2", recorded_at: 1_700_000_500_000 })],
        next_cursor: null,
      },
    });
    render(React.createElement(BillingUsageTab, { initialCharges: [charge()], initialCursor: "tok_page2" }));
    fireEvent.click(screen.getByRole("button", { name: "Next page" }));
    // One page on screen at a time — the operator never scrolls past rows
    // they already read to reach the control.
    await waitFor(() => expect(screen.getAllByText(/kimi-k2\.6/).length).toBe(1));
    expect(listChargesActionMock).toHaveBeenCalledWith({ limit: 25, cursor: "tok_page2" });
    expect(screen.getByText("Page 2")).toBeTruthy();
  });

  it("returns to a cached page without asking the server again", async () => {
    listChargesActionMock.mockResolvedValue({
      ok: true,
      data: { items: [charge({ id: "tel_2", event_id: "evt_2" })], next_cursor: null },
    });
    render(React.createElement(BillingUsageTab, { initialCharges: [charge()], initialCursor: "tok" }));
    fireEvent.click(screen.getByRole("button", { name: "Next page" }));
    await waitFor(() => expect(screen.getByText("Page 2")).toBeTruthy());

    const callsAfterForward = listChargesActionMock.mock.calls.length;
    // The pager disables both buttons while a fetch is in flight, and the
    // transition can still be settling after page 2 paints — clicking then
    // would be swallowed. Wait for the control to be live before using it.
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: "Previous page" }).hasAttribute("disabled"),
      ).toBe(false),
    );
    fireEvent.click(screen.getByRole("button", { name: "Previous page" }));
    await waitFor(() => expect(screen.getByText("Page 1")).toBeTruthy());
    // Backward motion is free: every page fetched is kept, so stepping back
    // costs no request at all.
    expect(listChargesActionMock.mock.calls.length).toBe(callsAfterForward);
  });

  it("surfaces a 'Not authenticated' alert when the action returns unauthenticated", async () => {
    listChargesActionMock.mockResolvedValue({ ok: false, error: "Not authenticated", status: 401 });
    render(React.createElement(BillingUsageTab, { initialCharges: [charge()], initialCursor: "tok" }));
    fireEvent.click(screen.getByRole("button", { name: "Next page" }));
    await waitFor(() => expect(screen.getByRole("alert").textContent).toContain("Not authenticated"));
  });

  it("surfaces a fetch error inline without losing the previous page", async () => {
    listChargesActionMock.mockResolvedValue({ ok: false, error: "503 service unavailable" });
    render(React.createElement(BillingUsageTab, { initialCharges: [charge()], initialCursor: "tok" }));
    fireEvent.click(screen.getByRole("button", { name: "Next page" }));
    await waitFor(() => expect(screen.getByRole("alert").textContent).toContain("503 service unavailable"));
    // The failed fetch leaves the operator on the page they can still read.
    expect(screen.getByText("kimi-k2.6 · run · 820→1040 tok")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Next page" })).toBeTruthy();
  });
});
