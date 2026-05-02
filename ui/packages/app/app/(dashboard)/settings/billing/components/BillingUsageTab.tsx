import { Badge, EmptyState } from "@usezombie/design-system";
import { ActivityIcon } from "lucide-react";
import type { GroupedEvent } from "../lib/groupCharges";

export type BillingUsageTabProps = {
  events: GroupedEvent[];
};

/**
 * Read-only Usage tab — newest-first per-event drain history. CSV export
 * + zombie/time filters land alongside Stripe billing in v2.1; the v2.0
 * surface is read-only on purpose so we can ship without a dependency on
 * a chart/filter primitive.
 */
export default function BillingUsageTab({ events }: BillingUsageTabProps) {
  if (events.length === 0) {
    return (
      <EmptyState
        icon={<ActivityIcon size={28} />}
        title="No billable events yet"
        description="Run a zombie event and its charges will appear here. Each event yields up to two rows (the receive gate and the stage execution)."
      />
    );
  }

  return (
    <div className="overflow-x-auto rounded-md border border-border animate-in fade-in-0 duration-200">
      <table className="w-full text-left text-xs">
        <thead className="border-b border-border bg-muted/50 text-muted-foreground">
          <tr>
            <th scope="col" className="p-3 font-medium">Event</th>
            <th scope="col" className="p-3 font-medium">Posture</th>
            <th scope="col" className="p-3 font-medium">Model</th>
            <th scope="col" className="p-3 font-medium text-right">In tok</th>
            <th scope="col" className="p-3 font-medium text-right">Out tok</th>
            <th scope="col" className="p-3 font-medium text-right">Receive</th>
            <th scope="col" className="p-3 font-medium text-right">Stage</th>
            <th scope="col" className="p-3 font-medium text-right">Total</th>
          </tr>
        </thead>
        <tbody>
          {events.map((e) => (
            <tr key={e.event_id} className="border-b border-border/50 last:border-0">
              <td className="p-3 font-mono">{e.event_id}</td>
              <td className="p-3">
                <Badge variant={e.posture === "byok" ? "cyan" : "default"}>
                  {e.posture}
                </Badge>
              </td>
              <td className="p-3 font-mono">{e.model}</td>
              <td className="p-3 text-right tabular-nums">{e.token_count_input ?? "—"}</td>
              <td className="p-3 text-right tabular-nums">{e.token_count_output ?? "—"}</td>
              <td className="p-3 text-right tabular-nums">{e.receive_cents}¢</td>
              <td className="p-3 text-right tabular-nums">{e.stage_cents}¢</td>
              <td className="p-3 text-right tabular-nums font-semibold">{e.total_cents}¢</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
