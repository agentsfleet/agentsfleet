"use client";

import { ActivityIcon } from "lucide-react";
import {
  Badge,
  DataTable,
  EmptyState,
  PAGINATION_KIND,
  Time,
  type DataTableColumn,
} from "@agentsfleet/design-system";
import { PROVIDER_MODE } from "@/lib/types";
import {
  describeCharge,
  formatChargeTimestamp,
  formatDollars,
  type ChargeRow,
} from "../lib/charges";
import { BILLING_PAGE_SIZE } from "@/lib/pagination/cursor-trail";
import { useUrlCursorPages } from "@/lib/pagination/use-url-cursor-pages";

export type BillingUsageTabProps = {
  initialCharges: ChargeRow[];
  initialCursor: string | null;
};

/**
 * Read-only Usage ledger — a terminal-native charge history (date · amount ·
 * type · description), newest-first, paged like every other table.
 * Each row is one raw telemetry charge (receive = gate-pass, stage = run); the
 * model + token detail rides the description column. Charges are deductions, so
 * amounts render negative. Pages are fetched by the Server Component above,
 * keyed by the cursor in the URL — this component holds no rows of its own.
 */


const COLUMNS: DataTableColumn<ChargeRow>[] = [
  {
    key: "date",
    header: "Date",
    sortValue: (c) => c.recorded_at,
    // The ledger keeps its approved "MMM DD, YYYY · HH:MM" string, now rendered
    // through Time so the cell carries the canonical <time datetime> ISO instant.
    // The label already IS the precise instant, so no hover tooltip is added.
    cell: (c) => (
      <Time
        value={new Date(c.recorded_at)}
        label={formatChargeTimestamp(c.recorded_at)}
        className="font-mono text-xs"
      />
    ),
  },
  {
    key: "amount",
    header: "Amount",
    numeric: true,
    sortValue: (c) => -c.credit_deducted_nanos,
    cell: (c) => (
      <span className="font-mono tabular-nums text-destructive">−{formatDollars(c.credit_deducted_nanos)}</span>
    ),
  },
  {
    key: "type",
    header: "Type",
    sortValue: (c) => c.posture,
    cell: (c) => (
      <Badge variant={c.posture === PROVIDER_MODE.self_managed ? "cyan" : "default"}>{c.posture}</Badge>
    ),
  },
  {
    key: "description",
    header: "Description",
    sortValue: (c) => describeCharge(c),
    cell: (c) => <span className="text-muted-foreground">{describeCharge(c)}</span>,
  },
];

export default function BillingUsageTab({ initialCharges, initialCursor }: BillingUsageTabProps) {
  // The page lives in the URL and the Server Component above already fetched
  // it, so this component holds no rows, no cache, and no fetch state.
  const feed = useUrlCursorPages(initialCursor);
  const charges = initialCharges;

  return (
    <div className="space-y-3">
      <DataTable
        columns={COLUMNS}
        rows={charges}
        rowKey={(c) => c.id}
        caption="usage history"
        empty={(
          <EmptyState
            icon={<ActivityIcon size={28} />}
            title="No charges yet"
            description="Charges appear once fleets run."
          />
        )}
        pagination={{
          kind: PAGINATION_KIND.page,
          page: feed.page,
          pageSize: BILLING_PAGE_SIZE,
          hasNext: feed.hasNext,
          totalLabel: "charges",
          onPageChange: feed.goToPage,
          isLoading: feed.isLoading,
        }}
      />
    </div>
  );
}
