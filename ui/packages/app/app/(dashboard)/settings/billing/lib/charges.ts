import { CHARGE_TYPE, NANOS_PER_USD, type TenantBillingChargesResponse } from "@/lib/types";

export type ChargeRow = TenantBillingChargesResponse["items"][number];

// Two-to-four decimal places — cents granularity, with sub-cent precision
// when traction rates ($0.001 stage, $0.0001 self-managed) need it.
const USD_FORMATTER = new Intl.NumberFormat("en-US", {
  style: "currency",
  currency: "USD",
  minimumFractionDigits: 2,
  maximumFractionDigits: 4,
});

/** Format a nanos amount as a USD string. */
export function formatDollars(nanos: number): string {
  return USD_FORMATTER.format(nanos / NANOS_PER_USD);
}

// recorded_at is epoch **milliseconds** (src/state/tenant_billing.zig `*_at_ms`).
// "Jun 15, 2026 · 17:33" mirrors the ledger date format in the approved mockup —
// the en-US date format already yields "MMM DD, YYYY"; the mono separator joins
// the 24h time. Two formatters avoid an untestable formatToParts fallback.
const DATE_FORMATTER = new Intl.DateTimeFormat("en-US", {
  month: "short",
  day: "2-digit",
  year: "numeric",
});
const TIME_FORMATTER = new Intl.DateTimeFormat("en-US", {
  hour: "2-digit",
  minute: "2-digit",
  hour12: false,
});

/** Format a charge's recorded_at (epoch ms) as "MMM DD, YYYY · HH:MM". */
export function formatChargeTimestamp(recordedAtMs: number): string {
  const at = new Date(recordedAtMs);
  return `${DATE_FORMATTER.format(at)} · ${TIME_FORMATTER.format(at)}`;
}

/**
 * Human description for a flat charge row — the telemetry has no free-text
 * field, so synthesise one from the operator-meaningful inputs (model +
 * tokens) and the charge phase. `receive` is the per-event gate-pass; `stage`
 * is the metered run. Keeps the model + token detail the prior grouped table
 * surfaced, now folded into the ledger's description column.
 */
export function describeCharge(row: ChargeRow): string {
  if (row.charge_type === CHARGE_TYPE.receive) {
    return `${row.model} · event gate-pass`;
  }
  const tokens =
    row.token_count_input != null && row.token_count_output != null
      ? ` · ${row.token_count_input}→${row.token_count_output} tok`
      : "";
  return `${row.model} · run${tokens}`;
}

export type ChargeSummary = {
  /** Sum of credit deducted across the loaded charge rows, in nanos. */
  spentNanos: number;
  /** Distinct billed events in the loaded window. */
  eventCount: number;
  /** Consumed fraction (spent / (balance + spent)) as a 0–100 percentage. */
  meterPct: number;
};

/**
 * Derive the balance meter + caption figures from the current balance and the
 * loaded charge rows — this surface is presentation-only (no period-total endpoint), so
 * "spent" / "events" reflect the loaded window. The meter shows the consumed
 * fraction (a usage bar that fills as you spend), floored to a hairline when
 * any spend exists so a non-zero balance never reads as a fully-empty track.
 */
export function summarizeCharges(rows: ChargeRow[], balanceNanos: number): ChargeSummary {
  const spentNanos = rows.reduce((sum, r) => sum + r.credit_deducted_nanos, 0);
  const eventCount = new Set(rows.map((r) => r.event_id)).size;
  // Only divide when there is spend, so the denominator (balance + spend) is
  // strictly positive — no zero-guard branch needed. No spend → empty track.
  const meterPct =
    spentNanos > 0
      ? Math.min(100, Math.max(1, (spentNanos / (balanceNanos + spentNanos)) * 100))
      : 0;
  return { spentNanos, eventCount, meterPct };
}
