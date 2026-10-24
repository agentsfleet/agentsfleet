import type { BillingActivity, BillingCharge, BillingReport, BillingTotals } from "@/lib/api/billing_report";
import { PROVIDER_MODE } from "@/lib/types";
import { BILLING_ACTIVITY_RUNTIME, BILLING_PERIOD_MONTH } from "@/lib/api/billing_report";

export const BILLING_AT = "2026-10-25T12:00:00Z";
export const BILLING_MODEL = "accounts/fireworks/models/minimax-m3p1-flash";
export const BILLING_MODEL_NAME = "MiniMax M3.1 Flash";
export const BILLING_FLEET = "01924f4e-0000-7000-8000-00000000c4a1";
export const BILLING_WORK = "01924f4e-0000-7000-8000-00000000c4a2";

export const BILLING_CHARGE_NANOS = "100000";
const BILLING_RUNTIME_MS = 1_000;

export const BILLING_SELECTION = `${BILLING_ACTIVITY_RUNTIME}:${BILLING_WORK}`;

export function billingTotals(overrides: Partial<BillingTotals> = {}): BillingTotals {
  return { charged_nanos: BILLING_CHARGE_NANOS, input_tokens: 40, output_tokens: 10, runtime_ms: BILLING_RUNTIME_MS,
    line_count: 1, activity_count: 1, unpriced_tokens: 0, ...overrides };
}

export function billingActivity(overrides: Partial<BillingActivity> = {}): BillingActivity {
  return { id: BILLING_WORK, activity_type: BILLING_ACTIVITY_RUNTIME, workspace_id: "workspace-1",
    fleet_id: BILLING_FLEET, fleet_name: "Payment alerts", recorded_at: BILLING_AT,
    totals: billingTotals(), models: [BILLING_MODEL], postures: [PROVIDER_MODE.platform], ...overrides };
}

export function billingCharge(overrides: Partial<BillingCharge> = {}): BillingCharge {
  return { id: "charge-1", sequence: 0, model: BILLING_MODEL, posture: PROVIDER_MODE.platform,
    charged_nanos: BILLING_CHARGE_NANOS, input_tokens: 40, output_tokens: 10, runtime_ms: BILLING_RUNTIME_MS,
    unpriced_tokens: 0, requested_at: BILLING_AT, settled_at: BILLING_AT, ...overrides };
}

export function billingReport(overrides: Partial<BillingReport> = {}): BillingReport {
  const totals = billingTotals();
  return { period: BILLING_PERIOD_MONTH, from_at: "2026-10-01T00:00:00Z", through_at: BILLING_AT,
    totals, spending: { by_fleet: [{ key: BILLING_FLEET, label: "Payment alerts", totals }],
      by_model: [{ key: BILLING_MODEL, label: null, totals }] },
    filters: { fleets: [{ id: BILLING_FLEET, name: "Payment alerts" }], models: [BILLING_MODEL] },
    activities: { items: [billingActivity()], total: 1, next_cursor: null }, ...overrides };
}

export const BILLING_CATALOGUE = {
  version: "fixture", total: 1, next_cursor: null,
  models: [{ id: BILLING_MODEL, provider: "fireworks", display_name: BILLING_MODEL_NAME,
    advanced: false, context_cap_tokens: 1_048_576, context_source_url: "https://example.test/model",
    context_verified_at: BILLING_AT, prices: [], input_nanos_per_mtok: "1",
    cached_input_nanos_per_mtok: "1", output_nanos_per_mtok: "1" }],
};
