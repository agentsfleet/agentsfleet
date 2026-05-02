import { describe, expect, it } from "vitest";
import {
  formatDollars,
  groupChargesByEvent,
} from "@/app/(dashboard)/settings/billing/lib/groupCharges";
import type { TenantBillingChargesResponse } from "@/lib/types";

type ChargeRow = TenantBillingChargesResponse["items"][number];

const RECEIVE: ChargeRow = {
  id: "tel_1",
  tenant_id: "t1",
  workspace_id: "w1",
  zombie_id: "z1",
  event_id: "evt_1",
  charge_type: "receive",
  posture: "platform",
  model: "kimi-k2.6",
  credit_deducted_cents: 1,
  token_count_input: null,
  token_count_output: null,
  wall_ms: null,
  recorded_at: 1_000_000,
};
const STAGE: ChargeRow = {
  ...RECEIVE,
  id: "tel_2",
  charge_type: "stage",
  credit_deducted_cents: 2,
  token_count_input: 820,
  token_count_output: 1040,
  wall_ms: 1500,
  recorded_at: 1_000_005,
};

describe("groupChargesByEvent", () => {
  it("returns an empty array for an empty input", () => {
    expect(groupChargesByEvent([])).toEqual([]);
  });

  it("groups receive+stage rows for one event into a single row with summed total", () => {
    const groups = groupChargesByEvent([STAGE, RECEIVE]); // out-of-order on purpose
    expect(groups).toHaveLength(1);
    const ev = groups[0]!;
    expect(ev.event_id).toBe("evt_1");
    expect(ev.receive_cents).toBe(1);
    expect(ev.stage_cents).toBe(2);
    expect(ev.total_cents).toBe(3);
    expect(ev.token_count_input).toBe(820);
    expect(ev.token_count_output).toBe(1040);
  });

  it("pins recorded_at to the earliest of the two rows (gate-pass moment)", () => {
    const groups = groupChargesByEvent([STAGE, RECEIVE]);
    expect(groups[0]?.recorded_at).toBe(RECEIVE.recorded_at);
  });

  it("sorts events newest-first", () => {
    const newer: ChargeRow = { ...RECEIVE, event_id: "evt_2", recorded_at: 2_000_000 };
    const groups = groupChargesByEvent([RECEIVE, newer]);
    expect(groups.map((g) => g.event_id)).toEqual(["evt_2", "evt_1"]);
  });

  it("handles a stage row with no matching receive (defensive — should never happen)", () => {
    const groups = groupChargesByEvent([STAGE]);
    expect(groups).toHaveLength(1);
    expect(groups[0]?.receive_cents).toBe(0);
    expect(groups[0]?.stage_cents).toBe(2);
    expect(groups[0]?.total_cents).toBe(2);
  });
});

describe("formatDollars", () => {
  it("formats 0 cents as $0.00", () => expect(formatDollars(0)).toBe("$0.00"));
  it("formats 471 cents as $4.71", () => expect(formatDollars(471)).toBe("$4.71"));
  it("formats 100 cents as $1.00", () => expect(formatDollars(100)).toBe("$1.00"));
  it("rounds half-cents to 2 decimals", () =>
    expect(formatDollars(1234)).toBe("$12.34"));
});
