import { describe, expect, it } from "vitest";
import {
  chargeAgentLabel,
  displayModelName,
  describeCharge,
  formatChargeAmount,
  formatChargeTimestamp,
  formatDollars,
  summarizeCharges,
  type ChargeRow,
} from "@/app/(dashboard)/settings/billing/lib/charges";
import { CHARGE_TYPE, NANOS_PER_USD, PROVIDER_MODE } from "@/lib/types";

// $0.001 — a representative sub-cent stage charge (the platform stage rate).
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

describe("formatDollars", () => {
  it("renders cents with sub-cent precision when present", () => {
    expect(formatDollars(290_000_000)).toBe("$0.29");
    expect(formatDollars(SAMPLE_CHARGE_NANOS)).toBe("$0.001");
    expect(formatDollars(0)).toBe("$0.00");
  });
});

describe("formatChargeAmount", () => {
  it("keeps zero debits honest and exposes a nonzero amount below display precision", () => {
    expect(formatChargeAmount(0)).toBe("$0.00");
    expect(formatChargeAmount(1)).toBe("<$0.0001");
    expect(formatChargeAmount(SAMPLE_CHARGE_NANOS)).toBe("−$0.001");
  });
});

describe("formatChargeTimestamp", () => {
  it("formats epoch-ms as 'MMM DD, YYYY · HH:MM' (timezone-agnostic structure)", () => {
    // Exact date/time shift by the runner's TZ; the structure is fixed.
    expect(formatChargeTimestamp(1_700_000_000_000)).toMatch(
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
    expect(describeCharge(charge())).toBe("Run · 820 input tokens · 1,040 output tokens");
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
  it("uses the fleet sigil and a readable model label", () => {
    expect(chargeAgentLabel(charge())).toMatch(/^Agent [A-Za-z]+-[0-9A-F]{4}$/);
    expect(displayModelName("deepseek-ai/DeepSeek-V4-Pro")).toBe("DeepSeek V4 Pro");
    expect(displayModelName("kimi-k2.6")).toBe("kimi k2.6");
  });
});

describe("summarizeCharges", () => {
  it("returns zeros for an empty window", () => {
    expect(summarizeCharges([], 4_710_000_000)).toEqual({
      spentNanos: 0,
      eventCount: 0,
      meterPct: 0,
    });
  });

  it("sums spend, counts distinct events, and computes the consumed fraction", () => {
    const rows = [
      charge({ id: "a", event_id: "evt_1", credit_deducted_nanos: 1, charge_type: CHARGE_TYPE.receive }),
      charge({ id: "b", event_id: "evt_1", credit_deducted_nanos: 289_999_999 }),
    ];
    const s = summarizeCharges(rows, 4_710_000_000);
    expect(s.spentNanos).toBe(290_000_000);
    expect(s.eventCount).toBe(1); // both rows are the same event
    // 290M / (4710M + 290M) = 5.8%
    expect(s.meterPct).toBeCloseTo(5.8, 5);
  });

  it("counts each distinct event once", () => {
    const rows = [
      charge({ id: "a", event_id: "evt_1" }),
      charge({ id: "b", event_id: "evt_2" }),
      charge({ id: "c", event_id: "evt_2" }),
    ];
    expect(summarizeCharges(rows, NANOS_PER_USD).eventCount).toBe(2);
  });

  it("floors a non-zero consumed fraction to a hairline so the track is never invisibly empty", () => {
    // Tiny spend against a huge balance → raw < 1% → floored to 1%.
    const s = summarizeCharges([charge({ credit_deducted_nanos: 1 })], 9_000_000_000_000);
    expect(s.spentNanos).toBe(1);
    expect(s.meterPct).toBe(1);
  });

  it("never exceeds 100% even when balance is exhausted", () => {
    const s = summarizeCharges([charge({ credit_deducted_nanos: 500_000_000 })], 0);
    expect(s.meterPct).toBe(100);
  });
});
