"use client";

import { useMemo, useState } from "react";
import { LayoutListIcon } from "lucide-react";
import {
  Badge,
  type BadgeVariant,
  DataTable,
  type DataTableColumn,
  EmptyState,
  PAGINATION_KIND,
  Time,
  WakePulse,
} from "@agentsfleet/design-system";
import { LEASE_OUTCOME, type RunnerLease, type RunnerLeaseResponse } from "@/lib/api/runners";
import { failureSentenceFor } from "@/lib/events/event-summary";
import { TABLE_PAGE_SIZE_OPTIONS } from "@/lib/pagination/cursor-trail";
import { useUrlCursorPages } from "@/lib/pagination/use-url-cursor-pages";
import { formatMs } from "@/lib/utils";
import { ReviewLease } from "./ReviewLease";
import {
  EXPIRED_ROW_DETAIL,
  EXPIRED_ROW_SENTENCE,
  LEASES_EMPTY_DESCRIPTION,
  LEASES_EMPTY_TITLE,
  LEASES_TABLE_LABEL,
  OUTCOME_LABELS,
  UNKNOWN_OUTCOME_SENTENCE,
} from "./runner-copy";

const VALUE_UNKNOWN = "—";
const SETTLED_OUTCOME_VARIANT: Partial<Record<RunnerLease["outcome"], BadgeVariant>> = {
  [LEASE_OUTCOME.succeeded]: "green",
  [LEASE_OUTCOME.failed]: "destructive",
  [LEASE_OUTCOME.expired]: "amber",
};

// The Leases landing view: the standard DataTable over the keyset lease read,
// live leases ordered first within the fetched page, every failed row reading
// the shared plain-English sentence — never the raw tag — and every row
// opening Review lease. The page itself is fetched by the Server Component
// for the cursor the URL names; this table only renders and pages it.
export function LeaseTable({ initial, pageSize }: { initial: RunnerLeaseResponse; pageSize: number }) {
  const [selected, setSelected] = useState<RunnerLease | null>(null);
  const feed = useUrlCursorPages(initial.next_cursor, pageSize);

  // Live work leads: within the page, every running row precedes every
  // settled row, each group keeping the server's newest-first order.
  const rows = useMemo(() => {
    const running = initial.items.filter((lease) => lease.outcome === LEASE_OUTCOME.running);
    const settled = initial.items.filter((lease) => lease.outcome !== LEASE_OUTCOME.running);
    return [...running, ...settled];
  }, [initial.items]);

  const columns = useMemo<DataTableColumn<RunnerLease>[]>(
    () => [
      {
        key: "fleet",
        header: "Fleet",
        cell: (lease) => (
          <span className="truncate font-mono text-sm">{lease.fleet_name ?? lease.fleet_id}</span>
        ),
      },
      {
        key: "work",
        header: "Work",
        cell: (lease) => (
          <span className="block min-w-0">
            <span className="block truncate text-muted-foreground">{lease.event_type}</span>
            <span className="block truncate text-label text-text-subtle">by {lease.actor}</span>
          </span>
        ),
      },
      {
        key: "when",
        header: "When",
        sortValue: (lease) => lease.created_at,
        cell: (lease) => (
          <Time
            value={new Date(lease.created_at)}
            format="relative"
            className="font-mono text-xs text-muted-foreground tabular-nums"
          />
        ),
      },
      {
        key: "took",
        header: "Took",
        numeric: true,
        hideOnMobile: true,
        cell: (lease) => (
          <span className="font-mono text-xs text-muted-foreground tabular-nums">
            {lease.wall_ms === null ? VALUE_UNKNOWN : formatMs(lease.wall_ms)}
          </span>
        ),
      },
      {
        key: "outcome",
        header: "Outcome",
        cell: (lease) => <OutcomeCell lease={lease} />,
      },
    ],
    [],
  );

  // The relative `Time` cells here, and Review lease's own, take tooltip
  // context from the root layout's single provider — see `app/layout.tsx`.
  return (
    <div>
      <DataTable
        caption={LEASES_TABLE_LABEL}
        columns={columns}
        rows={rows}
        rowKey={(lease) => lease.id}
        onRowClick={(lease) => setSelected(lease)}
        pagination={{
          kind: PAGINATION_KIND.page,
          page: feed.page,
          pageSize,
          hasNext: feed.hasNext,
          total: initial.total ?? undefined,
          totalLabel: "leases",
          onPageChange: feed.goToPage,
          pageSizeOptions: TABLE_PAGE_SIZE_OPTIONS,
          onPageSizeChange: feed.changePageSize,
          isLoading: feed.isLoading,
        }}
        empty={
          <EmptyState
            icon={<LayoutListIcon size={28} />}
            title={LEASES_EMPTY_TITLE}
            description={LEASES_EMPTY_DESCRIPTION}
          />
        }
      />
      <ReviewLease lease={selected} onOpenChange={() => setSelected(null)} />
    </div>
  );
}

// Outcome speaks the operator's language: a live lease pulses, a failed lease
// reads the shared failure sentence with the daemon's detail line under it, an
// expired lease states that the runner stopped renewing and the work was
// re-leased, and a missing event reads as not recorded — never a fabricated
// success.
function OutcomeCell({ lease }: { lease: RunnerLease }) {
  if (lease.outcome === LEASE_OUTCOME.running) {
    return (
      <span className="inline-flex items-center gap-md font-mono text-body-sm uppercase tracking-eyebrow text-pulse">
        <WakePulse live className="inline-block size-2 rounded-full bg-current" aria-hidden="true" />
        {OUTCOME_LABELS[lease.outcome]}
      </span>
    );
  }
  const variant = SETTLED_OUTCOME_VARIANT[lease.outcome] ?? "default";
  return (
    <span className="block min-w-0">
      <span className="flex flex-wrap items-center gap-md">
        <Badge variant={variant}>{OUTCOME_LABELS[lease.outcome]}</Badge>
        {lease.outcome === LEASE_OUTCOME.failed && lease.failure_label ? (
          <span className="text-muted-foreground">{failureSentenceFor(lease.failure_label)}</span>
        ) : null}
        {lease.outcome === LEASE_OUTCOME.expired ? (
          <span className="text-muted-foreground">{EXPIRED_ROW_SENTENCE}</span>
        ) : null}
        {lease.outcome === LEASE_OUTCOME.unknown ? (
          <span className="text-muted-foreground">{UNKNOWN_OUTCOME_SENTENCE}</span>
        ) : null}
      </span>
      {lease.outcome === LEASE_OUTCOME.failed && lease.failure_detail ? (
        <span className="mt-xs block truncate text-label text-text-subtle">{lease.failure_detail}</span>
      ) : null}
      {lease.outcome === LEASE_OUTCOME.expired ? (
        <span className="mt-xs block truncate text-label text-text-subtle">{EXPIRED_ROW_DETAIL}</span>
      ) : null}
    </span>
  );
}
