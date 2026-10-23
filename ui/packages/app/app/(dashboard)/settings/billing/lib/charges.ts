import { CHARGE_TYPE, type TenantBillingChargesResponse } from "@/lib/types";
import { agentDisplayName } from "@/lib/fleets/agent-label";
import { modelLabel } from "@/lib/models/display";
import { MS_PER_SECOND } from "@/lib/utils";
import { formatDollars, MIN_VISIBLE_NANOS, SUBVISIBLE_AMOUNT_LABEL } from "@/lib/money";
export { formatDollars } from "@/lib/money";

export type ChargeRow = TenantBillingChargesResponse["items"][number] & { model_display_name?: string | null };

const EVENT_RECEIVED_LABEL = "Event received";
const SETUP_CHAT_LABEL = "Setup chat";
const NO_TOKEN_USAGE_LABEL = "Token usage unavailable";
const TOKENS_LABEL = "tokens";
const TOKENS_IN = "input";
const TOKENS_OUT = "output";
const TOKEN_SPLIT_SEPARATOR = " · ";
const RUNTIME_SECONDS = new Intl.NumberFormat("en-US", {style: "unit", unit: "second", unitDisplay: "long", maximumFractionDigits: 3});
/** Human-readable debit: never render a misleading negative zero. */
export function formatChargeAmount(nanos: string): string {
  const amount = BigInt(nanos);
  if (amount === 0n) return formatDollars(nanos);
  if (amount < MIN_VISIBLE_NANOS) return SUBVISIBLE_AMOUNT_LABEL;
  return `−${formatDollars(nanos)}`;
}

/** Retained fleet identifiers keep billing labels stable after deletion. */
export function chargeAgentLabel(row: ChargeRow): string {
  if (row.charge_type === CHARGE_TYPE.authoring && row.fleet_id === null) return SETUP_CHAT_LABEL;
  return row.fleet_name && row.fleet_name !== row.fleet_id ? row.fleet_name : agentDisplayName(row.fleet_id);
}

/** Use the catalogue's spelling; preserve an unknown model's exact identifier. */
export function displayModelName(model: string, displayName?: string | null): string {
  return modelLabel(model, displayName);
}

// recorded_at is an RFC 3339 instant (`ChargeSummary::recorded_at` in
// rustd/crates/afd_api_wire/src/tenant.rs, read from the ledger's `created_at`).
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

/** Format a charge's recorded_at (an RFC 3339 instant) as "MMM DD, YYYY · HH:MM". */
export function formatChargeTimestamp(recordedAt: string): string {
  const at = new Date(recordedAt);
  return `${DATE_FORMATTER.format(at)} · ${TIME_FORMATTER.format(at)}`;
}

/** Recorded usage stays separate from the amount, which only the ledger supplies. */
export function chargeUsage(row: ChargeRow): {label: string; detail: string} {
  if (row.charge_type === CHARGE_TYPE.receive) {
    return {label: EVENT_RECEIVED_LABEL, detail: ""};
  }
  if (row.token_count_input === null || row.token_count_output === null) {
    return {label: NO_TOKEN_USAGE_LABEL, detail: ""};
  }
  if (row.token_count_input === 0 && row.token_count_output === 0) {
    return row.wall_ms !== null && row.wall_ms > 0
      ? {label: "Runtime", detail: RUNTIME_SECONDS.format(row.wall_ms / MS_PER_SECOND)}
      : {label: "0 tokens", detail: ""};
  }
  const total = row.token_count_input + row.token_count_output;
  const split = `${row.token_count_input.toLocaleString()} ${TOKENS_IN}${TOKEN_SPLIT_SEPARATOR}${row.token_count_output.toLocaleString()} ${TOKENS_OUT}`;
  return {label: `${total.toLocaleString()} ${TOKENS_LABEL}`, detail: split};
}

export function describeCharge(row: ChargeRow): string {
  const usage = chargeUsage(row);
  return usage.detail ? `${usage.label}${TOKEN_SPLIT_SEPARATOR}${usage.detail}` : usage.label;
}
