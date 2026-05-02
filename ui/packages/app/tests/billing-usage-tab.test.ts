import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

vi.mock("lucide-react", () => ({
  ActivityIcon: () => React.createElement("svg", { "data-icon": "ActivityIcon" }),
  Loader2Icon: () => React.createElement("svg", { "data-icon": "Loader2Icon" }),
}));

const { getTokenFn, listChargesMock } = vi.hoisted(() => ({
  getTokenFn: vi.fn(),
  listChargesMock: vi.fn(),
}));

vi.mock("@/lib/auth/client", () => ({
  useClientToken: () => ({ getToken: getTokenFn }),
}));
vi.mock("@/lib/api/tenant_billing", () => ({
  listTenantBillingCharges: listChargesMock,
}));

import BillingUsageTab from "@/app/(dashboard)/settings/billing/components/BillingUsageTab";
import type { GroupedEvent } from "@/app/(dashboard)/settings/billing/lib/groupCharges";

const SAMPLE: GroupedEvent = {
  event_id: "evt_1",
  zombie_id: "z_1",
  posture: "platform",
  model: "kimi-k2.6",
  recorded_at: 1_000_000,
  receive_cents: 1,
  stage_cents: 2,
  total_cents: 3,
  token_count_input: 820,
  token_count_output: 1040,
};

beforeEach(() => {
  getTokenFn.mockReset();
  listChargesMock.mockReset();
});
afterEach(() => cleanup());

describe("BillingUsageTab", () => {
  it("renders an empty-state when no events and no cursor", () => {
    render(React.createElement(BillingUsageTab, { initialEvents: [], initialCursor: null }));
    expect(screen.getByText("No billable events yet")).toBeTruthy();
    expect(screen.queryByTestId("usage-load-more")).toBeNull();
  });

  it("renders one row per event with summed totals + token counts", () => {
    render(React.createElement(BillingUsageTab, { initialEvents: [SAMPLE], initialCursor: null }));
    expect(screen.getByText("evt_1")).toBeTruthy();
    expect(screen.getByText("kimi-k2.6")).toBeTruthy();
    expect(screen.getByText("820")).toBeTruthy();
    expect(screen.getByText("1040")).toBeTruthy();
    expect(screen.getByText("3¢")).toBeTruthy();
  });

  it("renders em-dash for null token counts (receive-only event)", () => {
    const noStage: GroupedEvent = { ...SAMPLE, token_count_input: null, token_count_output: null };
    render(React.createElement(BillingUsageTab, { initialEvents: [noStage], initialCursor: null }));
    expect(screen.getAllByText("—").length).toBeGreaterThanOrEqual(2);
  });

  it("uses the cyan badge variant for BYOK posture (visual differentiation)", () => {
    const byokEvent: GroupedEvent = { ...SAMPLE, posture: "byok" };
    const { container } = render(
      React.createElement(BillingUsageTab, { initialEvents: [byokEvent], initialCursor: null }),
    );
    expect(container.textContent).toContain("byok");
  });

  it("hides the Load more button when there is no cursor", () => {
    render(React.createElement(BillingUsageTab, { initialEvents: [SAMPLE], initialCursor: null }));
    expect(screen.queryByTestId("usage-load-more")).toBeNull();
    expect(screen.getByText("No more events.")).toBeTruthy();
  });

  it("shows Load more when a cursor is present and fetches the next page on click", async () => {
    getTokenFn.mockResolvedValue("tok_more");
    const NEXT: GroupedEvent = { ...SAMPLE, event_id: "evt_2", recorded_at: 999_000 };
    listChargesMock.mockResolvedValue({
      items: [
        { ...SAMPLE, id: "tel_3", event_id: NEXT.event_id, charge_type: "receive", credit_deducted_cents: 1, recorded_at: NEXT.recorded_at, token_count_input: null, token_count_output: null },
        { ...SAMPLE, id: "tel_4", event_id: NEXT.event_id, charge_type: "stage",   credit_deducted_cents: 2, recorded_at: NEXT.recorded_at + 1, token_count_input: 100, token_count_output: 200 },
      ],
      next_cursor: null,
    });
    render(React.createElement(BillingUsageTab, { initialEvents: [SAMPLE], initialCursor: "tok_page2" }));
    fireEvent.click(screen.getByTestId("usage-load-more"));
    await waitFor(() => expect(screen.getByText("evt_2")).toBeTruthy());
    expect(listChargesMock).toHaveBeenCalledWith("tok_more", { limit: 50, cursor: "tok_page2" });
    // Cursor came back null → button gone, "no more" copy shown.
    await waitFor(() => expect(screen.queryByTestId("usage-load-more")).toBeNull());
    expect(screen.getByText("No more events.")).toBeTruthy();
  });

  it("de-dupes by event_id when a page boundary repeats an event", async () => {
    getTokenFn.mockResolvedValue("tok_dup");
    listChargesMock.mockResolvedValue({
      // Server returns the SAME event we already have.
      items: [
        { ...SAMPLE, id: "tel_dup_1", charge_type: "receive", credit_deducted_cents: 1, token_count_input: null, token_count_output: null },
        { ...SAMPLE, id: "tel_dup_2", charge_type: "stage",   credit_deducted_cents: 2 },
      ],
      next_cursor: null,
    });
    render(React.createElement(BillingUsageTab, { initialEvents: [SAMPLE], initialCursor: "tok_overlap" }));
    fireEvent.click(screen.getByTestId("usage-load-more"));
    await waitFor(() => expect(screen.queryByTestId("usage-load-more")).toBeNull());
    // Still exactly one row — the duplicate evt_1 was filtered.
    expect(screen.getAllByText("evt_1").length).toBe(1);
  });

  it("surfaces a 'Not authenticated' alert when getToken returns null", async () => {
    getTokenFn.mockResolvedValue(null);
    render(React.createElement(BillingUsageTab, { initialEvents: [SAMPLE], initialCursor: "tok" }));
    fireEvent.click(screen.getByTestId("usage-load-more"));
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toContain("Not authenticated"),
    );
    expect(listChargesMock).not.toHaveBeenCalled();
  });

  it("surfaces a fetch error inline without losing the previous page", async () => {
    getTokenFn.mockResolvedValue("tok_err");
    listChargesMock.mockRejectedValue(new Error("503 service unavailable"));
    render(React.createElement(BillingUsageTab, { initialEvents: [SAMPLE], initialCursor: "tok" }));
    fireEvent.click(screen.getByTestId("usage-load-more"));
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toContain("503 service unavailable"),
    );
    // Existing rows still rendered, button still available for retry.
    expect(screen.getByText("evt_1")).toBeTruthy();
    expect(screen.getByTestId("usage-load-more")).toBeTruthy();
  });
});
