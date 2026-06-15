import { CHARGE_TYPE, NANOS_PER_USD, type TenantBillingChargesResponse } from "@/lib/types";

export type ChargeRow = TenantBillingChargesResponse["items"][number];

export type GroupedEvent = {
  event_id: string;
  agent_id: string;
  posture: ChargeRow["posture"];
  model: string;
  recorded_at: number;
  receive_nanos: number;
  stage_nanos: number;
  total_nanos: number;
  token_count_input: number | null;
  token_count_output: number | null;
};

/**
 * Group raw charge rows by event_id. Each event yields up to two rows
 * (charge_type ∈ {receive, stage}); the dashboard renders one row per
 * event with both charges combined. Sorted newest-first by recorded_at.
 */
export function groupChargesByEvent(rows: ChargeRow[]): GroupedEvent[] {
  const byEvent = new Map<string, GroupedEvent>();
  for (const r of rows) {
    let entry = byEvent.get(r.event_id);
    if (!entry) {
      entry = {
        event_id: r.event_id,
        agent_id: r.agent_id,
        posture: r.posture,
        model: r.model,
        recorded_at: r.recorded_at,
        receive_nanos: 0,
        stage_nanos: 0,
        total_nanos: 0,
        token_count_input: null,
        token_count_output: null,
      };
      byEvent.set(r.event_id, entry);
    }
    if (r.charge_type === CHARGE_TYPE.receive) {
      entry.receive_nanos = r.credit_deducted_nanos;
    } else if (r.charge_type === CHARGE_TYPE.stage) {
      entry.stage_nanos = r.credit_deducted_nanos;
      entry.token_count_input = r.token_count_input;
      entry.token_count_output = r.token_count_output;
    }
    // Pin the event's recorded_at to the EARLIEST of receive/stage. The
    // newest-first sort below orders by gate-pass moment (when the event
    // entered the system), not by execution completion. Do NOT "fix" this
    // to use the latest timestamp — the dashboard's "when did this event
    // happen" semantic is gate-pass. Receive arrives before stage in the
    // worker write path, but this comparison tolerates either order for
    // replay / migration safety.
    if (r.recorded_at < entry.recorded_at) {
      entry.recorded_at = r.recorded_at;
    }
    entry.total_nanos = entry.receive_nanos + entry.stage_nanos;
  }
  // Stable secondary sort by event_id keeps order deterministic when two
  // events share a recorded_at (rare but possible at sub-ms precision).
  return Array.from(byEvent.values()).sort((a, b) => {
    const dt = b.recorded_at - a.recorded_at;
    return dt !== 0 ? dt : a.event_id.localeCompare(b.event_id);
  });
}

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
