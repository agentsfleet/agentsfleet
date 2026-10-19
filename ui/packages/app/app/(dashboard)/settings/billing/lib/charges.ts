import { CHARGE_TYPE, type TenantBillingChargesResponse } from "@/lib/types";
import { agentDisplayName } from "@/lib/fleets/agent-label";
import { formatDollars, MIN_VISIBLE_NANOS, SUBVISIBLE_AMOUNT_LABEL } from "@/lib/money";
export { formatDollars } from "@/lib/money";

export type ChargeRow = TenantBillingChargesResponse["items"][number];

const EVENT_RECEIVED_LABEL = "Event received";
const RUN_LABEL = "Run";
const SETUP_CHAT_LABEL = "Setup chat";
const NO_TOKEN_USAGE_LABEL = "No token usage recorded";
const TOKENS_LABEL = "tokens";
const TOKENS_IN = "in";
const TOKENS_OUT = "out";
const TOKEN_SPLIT_SEPARATOR = " · ";
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
  return agentDisplayName(row.fleet_id);
}

/** Strip the provider namespace and separators without changing model casing. */
export function displayModelName(model: string): string {
  const modelId = model.slice(model.lastIndexOf("/") + 1);
  return modelId.replaceAll(/[-_]/g, " ");
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

/**
 * Activity and recorded usage; the amount comes only from the saved debit.
 */
export function describeCharge(row: ChargeRow): string {
  if (row.charge_type === CHARGE_TYPE.receive) {
    return EVENT_RECEIVED_LABEL;
  }
  const label = row.charge_type === CHARGE_TYPE.authoring ? SETUP_CHAT_LABEL : RUN_LABEL;
  if (row.token_count_input === null || row.token_count_output === null) {
    return `${label} · ${NO_TOKEN_USAGE_LABEL}`;
  }
  if (row.token_count_input === 0 && row.token_count_output === 0) {
    return `${label} · ${NO_TOKEN_USAGE_LABEL}`;
  }
  // "2,075 input tokens · 1,112 output tokens" said "tokens" twice and led with
  // the least useful number. The total is what an operator scans for; the split
  // is what they check afterwards.
  const total = row.token_count_input + row.token_count_output;
  const split = `${row.token_count_input.toLocaleString()} ${TOKENS_IN}${TOKEN_SPLIT_SEPARATOR}${row.token_count_output.toLocaleString()} ${TOKENS_OUT}`;
  return `${label} · ${total.toLocaleString()} ${TOKENS_LABEL} · ${split}`;
}
