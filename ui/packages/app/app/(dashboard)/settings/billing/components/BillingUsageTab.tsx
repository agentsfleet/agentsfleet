"use client";

import { ActivityIcon } from "lucide-react";
import {
  DataTable,
  EmptyState,
  PAGINATION_KIND,
  Time,
  type DataTableColumn,
} from "@agentsfleet/design-system";
import {
  chargeAgentLabel,
  displayModelName,
  describeCharge,
  formatChargeTimestamp,
  formatChargeAmount,
  type ChargeRow,
} from "../lib/charges";
import { BILLING_PAGE_SIZE } from "@/lib/pagination/cursor-trail";
import { useUrlCursorPages } from "@/lib/pagination/use-url-cursor-pages";

export type BillingUsageTabProps = {
  initialCharges: ChargeRow[];
  initialCursor: string | null;
};

/**
 * Read-only Usage ledger. Each durable row states the agent identity, model,
 * activity and debit in operator language while retaining the backend's raw
 * charge granularity and URL-backed paging.
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
    key: "fleet",
    header: "Fleet and model",
    sortValue: (c) => chargeAgentLabel(c),
    cell: (c) => (
      <div className="flex min-w-48 flex-col">
        <span className="font-medium text-foreground">{chargeAgentLabel(c)}</span>
        <span className="text-muted-foreground">{displayModelName(c.model)}</span>
      </div>
    ),
  },
  {
    key: "activity",
    header: "Activity",
    sortValue: (c) => describeCharge(c),
    cell: (c) => <span className="text-muted-foreground">{describeCharge(c)}</span>,
  },
  {
    key: "amount",
    header: "Amount",
    numeric: true,
    sortValue: (c) => -c.credit_deducted_nanos,
    cell: (c) => (
      <span
        className="font-mono tabular-nums text-destructive data-[zero=true]:text-muted-foreground"
        data-zero={c.credit_deducted_nanos === 0 ? "true" : undefined}
      >
        {formatChargeAmount(c.credit_deducted_nanos)}
      </span>
    ),
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
        viewportClassName="max-h-72"
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
