// Wire-format constants for billing endpoints. Mirrors the canonical
// definitions in src/state/fleet_telemetry_store.zig (`ChargeType`) and
// src/state/tenant_provider.zig (`Mode`). Keep values verbatim — the API
// rejects anything else.

export const CHARGE_TYPE = Object.freeze({
  receive: "receive",
  stage: "stage",
});

export const PROVIDER_MODE = Object.freeze({
  platform: "platform",
  self_managed: "self_managed",
});

// 1¢ = 10_000_000 nanos. JS Number holds the canonical range
// (≤ 2^53 ≈ 9e15 nanos / ~$9M) without loss.
export const NANOS_PER_USD = 1_000_000_000;

// Rate constants — mirror src/state/tenant_billing.zig identifier-for-identifier
// (cross-tier parity rule). Bump these only as part of a paired rate change
// across Zig + ui/packages/website + ui/packages/app + ~/Projects/docs/snippets/rates.mdx.
// Held as Number; every value here fits in 2^53 so no precision loss.
export const STARTER_CREDIT_NANOS = 5 * NANOS_PER_USD;
export const EVENT_NANOS = 0;
// Per-second run rate ($0.0001/sec ≈ $0.36/hr), charged identically under both
// postures while a Fleet is actively running. Replaces the former flat
// per-stage fees.
export const RUN_NANOS_PER_SEC = 100_000;

// Promotional free-trial window: while `now_ms < FREE_TRIAL_END_MS`,
// the server's `compute_stage_charge` returns FREE_TRIAL_STAGE_NANOS
// regardless of posture / model / tokens. `agentsfleet doctor --json`'s
// `billing.free_trial` block surfaces the active state for clients.
// Live window state lives on agentsfleet.net/#pricing.
export const FREE_TRIAL_END_MS = 1_785_542_400_000; // 2026-08-01T00:00:00Z
export const FREE_TRIAL_STAGE_NANOS = 0;

// Two-to-four decimal places — cents granularity, with sub-cent precision
// when the per-second run rate ($0.0001/sec) needs it.
const USD_FORMATTER = new Intl.NumberFormat("en-US", {
  style: "currency",
  currency: "USD",
  minimumFractionDigits: 2,
  maximumFractionDigits: 4,
});

export function formatDollars(nanos: number | null | undefined): string {
  return USD_FORMATTER.format((nanos ?? 0) / NANOS_PER_USD);
}
