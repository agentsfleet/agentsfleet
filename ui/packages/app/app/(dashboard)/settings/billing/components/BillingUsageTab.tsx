"use client";

import { useState, useTransition } from "react";
import { ActivityIcon, Loader2Icon } from "lucide-react";
import {
  Alert,
  Badge,
  Button,
  DataTable,
  EmptyState,
  type DataTableColumn,
} from "@usezombie/design-system";
import { listTenantBillingChargesAction } from "../actions";
import { PROVIDER_MODE } from "@/lib/types";
import { formatDollars, groupChargesByEvent, type GroupedEvent } from "../lib/groupCharges";

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
 * pages are fetched via `listTenantBillingChargesAction`, a Server Action
 * that mints the api-template Bearer server-side via `getServerToken()`.
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
      <Badge variant={e.posture === PROVIDER_MODE.self_managed ? "cyan" : "default"}>{e.posture}</Badge>
    ),
  },
  { key: "model", header: "Model", cell: (e) => <span className="font-mono text-xs">{e.model}</span>, hideOnMobile: true },
  { key: "in_tok",  header: "In tok",  cell: (e) => e.token_count_input ?? "—", numeric: true, hideOnMobile: true },
  { key: "out_tok", header: "Out tok", cell: (e) => e.token_count_output ?? "—", numeric: true, hideOnMobile: true },
  { key: "receive", header: "Receive", cell: (e) => formatDollars(e.receive_nanos), numeric: true },
  { key: "stage",   header: "Stage",   cell: (e) => formatDollars(e.stage_nanos),   numeric: true },
  {
    key: "total",
    header: "Total",
    cell: (e) => <span className="font-semibold">{formatDollars(e.total_nanos)}</span>,
    numeric: true,
  },
];

export default function BillingUsageTab({
  initialEvents,
  initialCursor,
}: BillingUsageTabProps) {
  const [events, setEvents] = useState<GroupedEvent[]>(initialEvents);
  const [cursor, setCursor] = useState<string | null>(initialCursor);
  const [error, setError] = useState<string | null>(null);
  const [pending, startTransition] = useTransition();

  function loadMore() {
    if (!cursor) return;
    setError(null);
    startTransition(async () => {
      const result = await listTenantBillingChargesAction({ limit: PAGE_SIZE, cursor });
      if (!result.ok) {
        // Empty error string from the action would render as a blank
        // alert; mirror the `|| <default>` pattern used by every other
        // Server Action consumer.
        setError(result.error || "Failed to load more usage events");
        return;
      }
      const more = groupChargesByEvent(result.data.items);
      // De-dupe by event_id in case the page boundary repeats an event.
      const seen = new Set(events.map((e) => e.event_id));
      const fresh = more.filter((e) => !seen.has(e.event_id));
      setEvents([...events, ...fresh]);
      setCursor(result.data.next_cursor);
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
          <Alert variant="destructive" className="px-2 py-1">
            {error}
          </Alert>
        ) : null}
      </div>
    </div>
  );
}
