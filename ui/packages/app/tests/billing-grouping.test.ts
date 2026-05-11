import { describe, expect, it } from "vitest";
import {
  formatDollars,
  groupChargesByEvent,
} from "@/app/(dashboard)/settings/billing/lib/groupCharges";
import { CHARGE_TYPE, PROVIDER_MODE, type TenantBillingChargesResponse } from "@/lib/types";

type ChargeRow = TenantBillingChargesResponse["items"][number];

const RECEIVE: ChargeRow = {
  id: "tel_1",
  tenant_id: "t1",
  workspace_id: "w1",
  zombie_id: "z1",
  event_id: "evt_1",
  charge_type: CHARGE_TYPE.receive,
  posture: PROVIDER_MODE.platform,
  model: "kimi-k2.6",
  credit_deducted_nanos: 1,
  token_count_input: null,
  token_count_output: null,
  wall_ms: null,
  recorded_at: 1_000_000,
};
const STAGE: ChargeRow = {
  ...RECEIVE,
  id: "tel_2",
  charge_type: CHARGE_TYPE.stage,
  credit_deducted_nanos: 2,
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
    expect(ev.receive_nanos).toBe(1);
    expect(ev.stage_nanos).toBe(2);
    expect(ev.total_nanos).toBe(3);
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
    expect(groups[0]?.receive_nanos).toBe(0);
    expect(groups[0]?.stage_nanos).toBe(2);
    expect(groups[0]?.total_nanos).toBe(2);
  });

  it("ignores zero/null credit_deducted_nanos fallbacks", () => {
    const zeroRow: ChargeRow = { ...RECEIVE, credit_deducted_nanos: 0 };
    const groups = groupChargesByEvent([zeroRow]);
    expect(groups[0]?.receive_nanos).toBe(0);
    expect(groups[0]?.total_nanos).toBe(0);
  });

  it("treats null credit_deducted_nanos as 0 on a receive row (`?? 0` fallback)", () => {
    // Defensive — the API serializes the column as i64 NOT NULL, so a null
    // here is wire-shape drift, not a legitimate state. The `?? 0` keeps
    // grouping deterministic instead of propagating NaN into the dashboard.
    const nullReceive: ChargeRow = {
      ...RECEIVE,
      credit_deducted_nanos: null as unknown as ChargeRow["credit_deducted_nanos"],
    };
    const groups = groupChargesByEvent([nullReceive]);
    expect(groups[0]?.receive_nanos).toBe(0);
  });

  it("treats null credit_deducted_nanos as 0 on a stage row (`?? 0` fallback)", () => {
    const nullStage: ChargeRow = {
      ...STAGE,
      credit_deducted_nanos: null as unknown as ChargeRow["credit_deducted_nanos"],
    };
    const groups = groupChargesByEvent([nullStage]);
    expect(groups[0]?.stage_nanos).toBe(0);
  });

  it("skips updating recorded_at when the new row's timestamp is later", () => {
    // Verifies the `r.recorded_at < entry.recorded_at` branch — the second
    // row arrives later than the first, so entry.recorded_at must NOT change.
    const earlier: ChargeRow = { ...RECEIVE, event_id: "evt_x", recorded_at: 100 };
    const later: ChargeRow = { ...STAGE, event_id: "evt_x", recorded_at: 200 };
    const groups = groupChargesByEvent([earlier, later]);
    expect(groups[0]?.recorded_at).toBe(100);
  });

  it("orders ties by event_id (stable sort across engines)", () => {
    // Two events at the same recorded_at — the secondary event_id sort
    // pins ordering deterministically so the dashboard doesn't flicker.
    const a: ChargeRow = { ...RECEIVE, event_id: "evt_b", recorded_at: 5_000 };
    const b: ChargeRow = { ...RECEIVE, event_id: "evt_a", recorded_at: 5_000 };
    const groups = groupChargesByEvent([a, b]);
    expect(groups.map((g) => g.event_id)).toEqual(["evt_a", "evt_b"]);
  });

  it("ignores rows with an unknown charge_type (defensive)", () => {
    const weird = { ...RECEIVE, charge_type: "unknown" as ChargeRow["charge_type"] };
    const groups = groupChargesByEvent([weird]);
    expect(groups[0]?.receive_nanos).toBe(0);
    expect(groups[0]?.stage_nanos).toBe(0);
  });
});

describe("formatDollars", () => {
  // Pin tests for the nanos-based dollar formatter. 1¢ = 10_000_000 nanos.
  it("formats 0 nanos as $0.00", () => expect(formatDollars(0)).toBe("$0.00"));
  it("formats 4_710_000_000 nanos as $4.71", () =>
    expect(formatDollars(4_710_000_000)).toBe("$4.71"));
  it("formats 1_000_000_000 nanos as $1.00", () =>
    expect(formatDollars(1_000_000_000)).toBe("$1.00"));
  it("formats 12_340_000_000 nanos as $12.34", () =>
    expect(formatDollars(12_340_000_000)).toBe("$12.34"));
  it("renders sub-cent traction rate $0.001 (1_000_000 nanos)", () =>
    expect(formatDollars(1_000_000)).toBe("$0.001"));
  it("renders sub-cent traction rate $0.0001 (100_000 nanos)", () =>
    expect(formatDollars(100_000)).toBe("$0.0001"));
});
