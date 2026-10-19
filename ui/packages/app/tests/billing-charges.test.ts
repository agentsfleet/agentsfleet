import { describe, expect, it } from "vitest";
import {
  chargeAgentLabel,
  displayModelName,
  describeCharge,
  formatChargeAmount,
  formatChargeTimestamp,
  formatDollars,
  type ChargeRow,
} from "@/app/(dashboard)/settings/billing/lib/charges";
import { CHARGE_TYPE, NANOS_PER_USD, PROVIDER_MODE } from "@/lib/types";

// $0.001 — a representative sub-cent stage charge (the platform stage rate).
const SAMPLE_CHARGE_NANOS = "1000000";

function charge(over: Partial<ChargeRow> = {}): ChargeRow {
  return {
    id: "tel_1",
    tenant_id: "t_1",
    workspace_id: "w_1",
    fleet_id: "z_1",
    fleet_name: "z_1",
    event_id: "evt_1",
    authoring_attempt_id: null,
    charge_type: CHARGE_TYPE.stage,
    posture: PROVIDER_MODE.platform,
    model: "kimi-k2.6",
    credit_deducted_nanos: SAMPLE_CHARGE_NANOS,
    token_count_input: 820,
    token_count_output: 1040,
    wall_ms: 3000,
    recorded_at: "2023-11-14T22:13:20Z",
    ...over,
  };
}

describe("formatDollars", () => {
  it("renders cents with sub-cent precision when present", () => {
    expect(formatDollars("290000000")).toBe("$0.29");
    expect(formatDollars(SAMPLE_CHARGE_NANOS)).toBe("$0.001");
    expect(formatDollars("0")).toBe("$0.00");
  });
});

describe("formatChargeAmount", () => {
  it("keeps zero debits honest and exposes a nonzero amount below display precision", () => {
    expect(formatChargeAmount("0")).toBe("$0.00");
    expect(formatChargeAmount("1")).toBe("<$0.0001");
    expect(formatChargeAmount(SAMPLE_CHARGE_NANOS)).toBe("−$0.001");
  });
});

describe("formatChargeTimestamp", () => {
  it("formats an RFC 3339 instant as 'MMM DD, YYYY · HH:MM' (timezone-agnostic structure)", () => {
    // Exact date/time shift by the runner's TZ; the structure is fixed.
    expect(formatChargeTimestamp("2023-11-14T22:13:20Z")).toMatch(
      /^[A-Z][a-z]{2} \d{2}, \d{4} · \d{2}:\d{2}$/,
    );
  });
});

describe("describeCharge", () => {
  it("describes a receive charge as an event receipt", () => {
    expect(describeCharge(charge({ charge_type: CHARGE_TYPE.receive }))).toBe(
      "Event received",
    );
  });

  it("describes a run with explicit input and output token counts", () => {
    expect(describeCharge(charge())).toBe("Run · 1,860 tokens · 820 in · 1,040 out");
  });

  it("explains when a run recorded no token usage", () => {
    expect(
      describeCharge(charge({ token_count_input: null, token_count_output: null })),
    ).toBe("Run · No token usage recorded");
  });

  it("explains when a run records explicit zero token counts", () => {
    expect(describeCharge(charge({ token_count_input: 0, token_count_output: 0 }))).toBe(
      "Run · No token usage recorded",
    );
  });
});

describe("charge identity", () => {
  it("names setup costs before a fleet exists without marking them deleted", () => {
    const row = charge({ fleet_id: null, event_id: null, charge_type: CHARGE_TYPE.authoring });
    expect(chargeAgentLabel(row)).toBe("Setup chat");
    expect(describeCharge(row)).toBe("Setup chat · 1,860 tokens · 820 in · 1,040 out");
    expect(describeCharge({ ...row, token_count_input: null })).toBe("Setup chat · No token usage recorded");
  });

  it("uses the fleet sigil and a readable model label", () => {
    expect(chargeAgentLabel(charge())).toMatch(/^AGENT [A-Z]+-[0-9A-F]{4}$/);
    expect(displayModelName("deepseek-ai/DeepSeek-V4-Pro")).toBe("DeepSeek V4 Pro");
    expect(displayModelName("kimi-k2.6")).toBe("kimi k2.6");
  });

  it("uses only the callsign even when the charge retains the fleet name", () => {
    expect(chargeAgentLabel(charge({ fleet_name: "deploy-bot" }))).toMatch(
      /^AGENT [A-Z]+-[0-9A-F]{4}$/,
    );
  });

  // A fleet purged before its charge landed: the server stores the fleet's own
  // identifier as the name, and the label shows the callsign alone rather
  // than that identifier.
  it("shows the callsign alone when the charge captured no name", () => {
    const label = chargeAgentLabel(charge({ fleet_id: "z_1", fleet_name: "z_1" }));
    expect(label).toMatch(/^AGENT [A-Z]+-[0-9A-F]{4}$/);
    expect(label).not.toContain("z_1");
  });
});

// The dashboard holds the nanos denominator and no rate. These cover what that
// buys: a figure the server sent is the figure rendered, and the denominator
// stays exact across the whole range a balance can occupy.

// Deliberately inconsistent with any per-second rate: 3_000 ms of runtime at
// the daemon's run rate would be 300_000_000 nanos, not this. A client that
// recomputed a run fee from wall_ms would report that instead of what the
// ledger row actually says was charged.
const SERVED_COST_NANOS = "7111111";

// The largest balance the product's own copy claims to hold without loss.
const NINE_MILLION_USD = 9_000_000;

describe("spend is reported from the served figure, never recomputed", () => {
  it("renders a charge the server sent even when no rate could have produced it", () => {
    expect(formatChargeAmount(SERVED_COST_NANOS)).toBe("−$0.0071");
  });
});

describe("the nanos denominator stays exact across a balance's range", () => {
  it("formats the largest balance the wire format claims to carry", () => {
    expect(formatDollars(String(NINE_MILLION_USD * NANOS_PER_USD))).toBe("$9,000,000.00");
  });

  it("keeps that balance inside the safe-integer range", () => {
    expect(NINE_MILLION_USD * NANOS_PER_USD).toBeLessThan(Number.MAX_SAFE_INTEGER);
  });
});
