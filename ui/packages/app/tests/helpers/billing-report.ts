import type { BillingCountedTotals, BillingReport, BillingTotals } from "@/lib/api/billing_report";
import { BILLING_PERIOD_MONTH } from "@/lib/api/billing-report-values";

export const BILLING_AT = "2026-10-25T12:00:00Z";
export const BILLING_MODEL = "accounts/fireworks/models/minimax-m3p1-flash";
export const BILLING_MODEL_NAME = "MiniMax M3.1 Flash";
export const BILLING_FLEET = "01924f4e-0000-7000-8000-00000000c4a1";

export const BILLING_CHARGE_NANOS = "100000";
const BILLING_RUNTIME_MS = 1_000;

/** Totals for the period or a fleet: the groups the server counts activities in. */
export function billingTotals(overrides: Partial<BillingCountedTotals> = {}): BillingCountedTotals {
  return { charged_nanos: BILLING_CHARGE_NANOS, input_tokens: 40, output_tokens: 10, runtime_ms: BILLING_RUNTIME_MS,
    line_count: 1, activity_count: 1, unpriced_tokens: 0, ...overrides };
}

/** Totals for a day or a model, which the server serves without an activity count. */
export function billingRowTotals(overrides: Partial<BillingTotals> = {}): BillingTotals {
  return { charged_nanos: BILLING_CHARGE_NANOS, input_tokens: 40, output_tokens: 10, runtime_ms: BILLING_RUNTIME_MS,
    line_count: 1, unpriced_tokens: 0, ...overrides };
}

export function billingReport(overrides: Partial<BillingReport> = {}): BillingReport {
  const totals = overrides.totals ?? billingTotals();
  const { activity_count: _counted, ...row } = totals;
  const from = overrides.from_at ?? "2026-10-01T00:00:00Z";
  const through = overrides.through_at ?? BILLING_AT;
  const start = Date.parse(`${from.slice(0, 10)}T00:00:00Z`);
  const end = Date.parse(`${through.slice(0, 10)}T00:00:00Z`);
  const dayMs = 86_400_000;
  const count = (end - start) / dayMs + 1;
  return { period: BILLING_PERIOD_MONTH, from_at: from, through_at: through,
    totals, daily: Array.from({length: count}, (_, index) => ({day: new Date(start + index * dayMs).toISOString().slice(0, 10),
      totals: index === count - 1 ? row : billingRowTotals({charged_nanos: "0", input_tokens: 0, output_tokens: 0, runtime_ms: 0, line_count: 0})})),
    spending: { by_fleet: [{ key: BILLING_FLEET, label: "Payment alerts", models: [BILLING_MODEL], totals }],
      by_model: [{ key: BILLING_MODEL, label: null, totals: row }] },
    filters: { fleets: [{ id: BILLING_FLEET, name: "Payment alerts" }] }, ...overrides };
}

export const BILLING_CATALOGUE = {
  version: "fixture", total: 1, next_cursor: null,
  models: [{ id: BILLING_MODEL, provider: "fireworks", display_name: BILLING_MODEL_NAME,
    advanced: false, context_cap_tokens: 1_048_576, context_source_url: "https://example.test/model",
    context_verified_at: BILLING_AT, prices: [], input_nanos_per_mtok: "1",
    cached_input_nanos_per_mtok: "1", output_nanos_per_mtok: "1" }],
};
