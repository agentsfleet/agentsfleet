"use client";

import { useState, useTransition } from "react";
import { ActivityIcon, Loader2Icon } from "lucide-react";
import { Badge, Button, EmptyState } from "@usezombie/design-system";
import { DataTable, type DataTableColumn } from "@/components/ui/data-table";
import { useClientToken } from "@/lib/auth/client";
import { listTenantBillingCharges } from "@/lib/api/tenant_billing";
import { groupChargesByEvent, type GroupedEvent } from "../lib/groupCharges";

export type BillingUsageTabProps = {
  initialEvents: GroupedEvent[];
  initialCursor: string | null;
};

/**
 * Read-only Usage tab — newest-first per-event drain history with
 * cursor-based "Load more" pagination. CSV export + zombie/time filters
 * land alongside Stripe billing in v2.1; the v2.0 surface stays read-only
 * on purpose so we can ship without a dependency on a chart/filter
 * primitive.
 *
 * Initial events + cursor come from the server-rendered page; subsequent
 * pages are fetched client-side with the bearer token from useClientToken.
 * `limit * 2` is intentional: each event yields up to two rows (receive +
 * stage), so we ask for double the rows we'll surface as events.
 */
const PAGE_SIZE = 50;

const COLUMNS: DataTableColumn<GroupedEvent>[] = [
  { key: "event_id", header: "Event", cell: (e) => <span className="font-mono text-xs">{e.event_id}</span> },
  {
    key: "posture",
    header: "Posture",
    cell: (e) => (
      <Badge variant={e.posture === "byok" ? "cyan" : "default"}>{e.posture}</Badge>
    ),
  },
  { key: "model", header: "Model", cell: (e) => <span className="font-mono text-xs">{e.model}</span>, hideOnMobile: true },
  { key: "in_tok",  header: "In tok",  cell: (e) => e.token_count_input ?? "—", numeric: true, hideOnMobile: true },
  { key: "out_tok", header: "Out tok", cell: (e) => e.token_count_output ?? "—", numeric: true, hideOnMobile: true },
  { key: "receive", header: "Receive", cell: (e) => `${e.receive_cents}¢`, numeric: true },
  { key: "stage",   header: "Stage",   cell: (e) => `${e.stage_cents}¢`,   numeric: true },
  {
    key: "total",
    header: "Total",
    cell: (e) => <span className="font-semibold">{e.total_cents}¢</span>,
    numeric: true,
  },
];

export default function BillingUsageTab({
  initialEvents,
  initialCursor,
}: BillingUsageTabProps) {
  const { getToken } = useClientToken();
  const [events, setEvents] = useState<GroupedEvent[]>(initialEvents);
  const [cursor, setCursor] = useState<string | null>(initialCursor);
  const [error, setError] = useState<string | null>(null);
  const [pending, startTransition] = useTransition();

  function loadMore() {
    if (!cursor) return;
    setError(null);
    startTransition(async () => {
      const token = await getToken();
      if (!token) {
        setError("Not authenticated");
        return;
      }
      try {
        const resp = await listTenantBillingCharges(token, {
          limit: PAGE_SIZE,
          cursor,
        });
        const more = groupChargesByEvent(resp.items);
        // De-dupe by event_id in case the page boundary repeats an event.
        const seen = new Set(events.map((e) => e.event_id));
        const fresh = more.filter((e) => !seen.has(e.event_id));
        setEvents([...events, ...fresh]);
        setCursor(resp.next_cursor);
      } catch (err) {
        setError(err instanceof Error ? err.message : String(err));
      }
    });
  }

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
    <div className="space-y-3 animate-in fade-in-0 duration-200">
      <DataTable
        columns={COLUMNS}
        rows={events}
        rowKey={(e) => e.event_id}
        caption="Credit-pool charges"
      />
      <div className="flex items-center gap-3 text-xs">
        {cursor ? (
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={loadMore}
            disabled={pending}
            data-testid="usage-load-more"
          >
            {pending ? <Loader2Icon size={12} className="animate-spin" aria-hidden /> : null}
            Load more
          </Button>
        ) : (
          <span className="text-muted-foreground">No more events.</span>
        )}
        {error ? (
          <span
            role="alert"
            className="rounded-md border border-destructive/40 bg-destructive/10 px-2 py-1 text-destructive animate-in fade-in-0 duration-200"
          >
            {error}
          </span>
        ) : null}
      </div>
    </div>
  );
}
