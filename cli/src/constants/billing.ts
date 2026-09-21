// Wire-format constants for billing endpoints. Mirrors the canonical
// definitions the daemon serves. Keep values verbatim — the API rejects
// anything else.

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
