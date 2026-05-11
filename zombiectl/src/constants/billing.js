// Wire-format constants for billing endpoints. Mirrors the canonical
// definitions in src/state/zombie_telemetry_store.zig (`ChargeType`) and
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
export const STAGE_PLATFORM_NANOS = 1_000_000;
export const STAGE_SELF_MANAGED_NANOS = 100_000;

// Two-to-four decimal places — cents granularity, with sub-cent precision
// when traction rates ($0.001 stage, $0.0001 self-managed) need it.
const USD_FORMATTER = new Intl.NumberFormat("en-US", {
  style: "currency",
  currency: "USD",
  minimumFractionDigits: 2,
  maximumFractionDigits: 4,
});

export function formatDollars(nanos) {
  return USD_FORMATTER.format((nanos ?? 0) / NANOS_PER_USD);
}
