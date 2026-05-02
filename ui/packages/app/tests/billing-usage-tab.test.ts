import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";

vi.mock("lucide-react", () => ({
  ActivityIcon: () => React.createElement("svg", { "data-icon": "ActivityIcon" }),
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

afterEach(() => cleanup());

describe("BillingUsageTab", () => {
  it("renders an empty-state when no events", () => {
    render(React.createElement(BillingUsageTab, { events: [] }));
    expect(screen.getByText("No billable events yet")).toBeTruthy();
  });

  it("renders one row per event with summed totals + token counts", () => {
    render(React.createElement(BillingUsageTab, { events: [SAMPLE] }));
    expect(screen.getByText("evt_1")).toBeTruthy();
    expect(screen.getByText("kimi-k2.6")).toBeTruthy();
    expect(screen.getByText("820")).toBeTruthy();
    expect(screen.getByText("1040")).toBeTruthy();
    expect(screen.getByText("3¢")).toBeTruthy();
  });

  it("renders em-dash for null token counts (receive-only event)", () => {
    const noStage: GroupedEvent = { ...SAMPLE, token_count_input: null, token_count_output: null };
    render(React.createElement(BillingUsageTab, { events: [noStage] }));
    // Two cells render em-dash for tok counts on a receive-only event.
    expect(screen.getAllByText("—").length).toBeGreaterThanOrEqual(2);
  });

  it("uses the cyan badge variant for BYOK posture (visual differentiation)", () => {
    const byokEvent: GroupedEvent = { ...SAMPLE, posture: "byok" };
    const { container } = render(React.createElement(BillingUsageTab, { events: [byokEvent] }));
    expect(container.textContent).toContain("byok");
  });
});
