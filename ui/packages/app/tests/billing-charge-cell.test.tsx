import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render } from "@testing-library/react";

vi.mock("lucide-react", () => ({
  ActivityIcon: () => React.createElement("svg", { "data-icon": "ActivityIcon" }),
  Loader2Icon: () => React.createElement("svg", { "data-icon": "Loader2Icon" }),
}));

vi.mock("@/app/(dashboard)/settings/billing/actions", () => ({
  listTenantBillingChargesAction: vi.fn(),
}));

import BillingUsageTab from "@/app/(dashboard)/settings/billing/components/BillingUsageTab";
import {
  formatChargeTimestamp,
  type ChargeRow,
} from "@/app/(dashboard)/settings/billing/lib/charges";
import { CHARGE_TYPE, PROVIDER_MODE } from "@/lib/types";

// A fixed epoch-ms instant so the ISO datetime attribute is deterministic.
const RECORDED_AT_MS = 1_700_000_000_000;
/** Fixture-only charge amount; nothing asserts on it. */
const CREDIT_DEDUCTED_NANOS = 1_000_000;

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
    credit_deducted_nanos: CREDIT_DEDUCTED_NANOS,
    token_count_input: 820,
    token_count_output: 1040,
    wall_ms: 3000,
    recorded_at: RECORDED_AT_MS,
    ...over,
  };
}

afterEach(() => cleanup());

describe("BillingUsageTab charge cell", () => {
  it("test_billing_charge_cell_time_label", () => {
    const { container } = render(
      React.createElement(BillingUsageTab, {
        initialCharges: [charge()],
        initialCursor: null,
      }),
    );

    const time = container.querySelector("time");
    expect(time).not.toBeNull();
    // Visible text is still the approved ledger string ("MMM DD, YYYY · HH:MM").
    expect(time?.textContent).toBe(formatChargeTimestamp(RECORDED_AT_MS));
    // The datetime attribute is the canonical ISO instant.
    expect(time?.getAttribute("datetime")).toBe(
      new Date(RECORDED_AT_MS).toISOString(),
    );
  });
});
