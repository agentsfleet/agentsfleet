import { CHARGE_TYPE, NANOS_PER_USD, type TenantBillingChargesResponse } from "@/lib/types";
import { deriveFleetIdentity } from "@/app/(dashboard)/w/[workspaceId]/fleets/components/fleetIdentity";

export type ChargeRow = TenantBillingChargesResponse["items"][number];

// Two-to-four decimal places — cents granularity, with sub-cent precision
// when traction rates ($0.001 stage, $0.0001 self-managed) need it.
const USD_FORMATTER = new Intl.NumberFormat("en-US", {
  style: "currency",
  currency: "USD",
  minimumFractionDigits: 2,
  maximumFractionDigits: 4,
});
const AGENT_PREFIX = "Agent";
const EVENT_RECEIVED_LABEL = "Event received";
const RUN_LABEL = "Run";
const NO_TOKEN_USAGE_LABEL = "No token usage recorded";
const MIN_VISIBLE_DEBIT_NANOS = 50_000;
const SUBVISIBLE_DEBIT_LABEL = "<$0.0001";

/** Format a nanos amount as a USD string. */
export function formatDollars(nanos: number): string {
  return USD_FORMATTER.format(nanos / NANOS_PER_USD);
}

/** Human-readable debit: never render a misleading negative zero. */
export function formatChargeAmount(nanos: number): string {
  if (nanos === 0) return formatDollars(0);
  if (nanos < MIN_VISIBLE_DEBIT_NANOS) return SUBVISIBLE_DEBIT_LABEL;
  return `−${formatDollars(nanos)}`;
}

/** Stable fleet identity is available on every telemetry row. */
export function chargeAgentLabel(row: ChargeRow): string {
  return `${AGENT_PREFIX} ${deriveFleetIdentity(row.fleet_id).callsign}`;
}

/** Strip the provider namespace and separators without changing model casing. */
export function displayModelName(model: string): string {
  const modelId = model.slice(model.lastIndexOf("/") + 1);
  return modelId.replaceAll(/[-_]/g, " ");
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
 * Human activity summary for a flat charge row. A billing record has no
 * provider action or configured fleet name, so this stays precise about the
 * information it does have: receipt or run, plus token use when recorded.
 */
export function describeCharge(row: ChargeRow): string {
  if (row.charge_type === CHARGE_TYPE.receive) {
    return EVENT_RECEIVED_LABEL;
  }
  if (row.token_count_input === null || row.token_count_output === null) {
    return `${RUN_LABEL} · ${NO_TOKEN_USAGE_LABEL}`;
  }
  if (row.token_count_input === 0 && row.token_count_output === 0) {
    return `${RUN_LABEL} · ${NO_TOKEN_USAGE_LABEL}`;
  }
  return `${RUN_LABEL} · ${row.token_count_input.toLocaleString()} input tokens · ${row.token_count_output.toLocaleString()} output tokens`;
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
