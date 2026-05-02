import type { TenantBillingChargesResponse } from "@/lib/types";

export type ChargeRow = TenantBillingChargesResponse["items"][number];

export type GroupedEvent = {
  event_id: string;
  zombie_id: string;
  posture: ChargeRow["posture"];
  model: string;
  recorded_at: number;
  receive_cents: number;
  stage_cents: number;
  total_cents: number;
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
        zombie_id: r.zombie_id,
        posture: r.posture,
        model: r.model,
        recorded_at: r.recorded_at,
        receive_cents: 0,
        stage_cents: 0,
        total_cents: 0,
        token_count_input: null,
        token_count_output: null,
      };
      byEvent.set(r.event_id, entry);
    }
    if (r.charge_type === "receive") {
      entry.receive_cents = r.credit_deducted_cents ?? 0;
    } else if (r.charge_type === "stage") {
      entry.stage_cents = r.credit_deducted_cents ?? 0;
      entry.token_count_input = r.token_count_input;
      entry.token_count_output = r.token_count_output;
    }
    if (r.recorded_at != null && (entry.recorded_at == null || r.recorded_at < entry.recorded_at)) {
      entry.recorded_at = r.recorded_at;
    }
    entry.total_cents = entry.receive_cents + entry.stage_cents;
  }
  return Array.from(byEvent.values()).sort((a, b) => (b.recorded_at ?? 0) - (a.recorded_at ?? 0));
}

/** Format cents as "$X.XX". */
export function formatDollars(cents: number): string {
  return `$${(cents / 100).toFixed(2)}`;
}
