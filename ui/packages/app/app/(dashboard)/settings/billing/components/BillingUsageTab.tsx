"use client";

import { TIME_LABEL } from "@/lib/time-labels";
import { ActivityIcon } from "lucide-react";
import {
  DataTable,
  EmptyState,
  EYEBROW_CLASS,
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
import { TABLE_PAGE_SIZE_OPTIONS } from "@/lib/pagination/cursor-trail";
import { useUrlCursorPages } from "@/lib/pagination/use-url-cursor-pages";

export type BillingUsageTabProps = {
  initialCharges: ChargeRow[];
  initialCursor: string | null;
  pageSize: number;
};

/**
 * Read-only Usage ledger. Each durable row states the agent identity, model,
 * activity and debit in operator language while retaining the backend's raw
 * charge granularity and URL-backed paging.
 */

const COLUMNS: DataTableColumn<ChargeRow>[] = [
  {
    key: "fleet", primary: true,
    header: "Fleet",
    sortValue: (c) => chargeAgentLabel(c),
    cell: (c) => <ChargeIdentityCell charge={c} />,
  },
  {
    key: "model",
    header: "Model",
    hideOnMobile: true,
    sortValue: (c) => displayModelName(c.model),
    cell: (c) => (
      <span className="text-muted-foreground">{displayModelName(c.model)}</span>
    ),
  },
  {
    key: "activity",
    header: "Activity",
    hideOnMobile: true,
    sortValue: (c) => describeCharge(c),
    cell: (c) => (
      <span className="text-muted-foreground">{describeCharge(c)}</span>
    ),
  },
  {
    key: "created",
    header: TIME_LABEL.created,
    hideOnMobile: true,
    sortValue: (c) => Date.parse(c.recorded_at),
    // The ledger keeps its approved "MMM DD, YYYY · HH:MM" string, now rendered
    // through Time so the cell carries the canonical <time datetime> ISO instant.
    // The label already IS the precise instant, so no hover tooltip is added.
    cell: (c) => (
      <Time
        value={c.recorded_at}
        label={formatChargeTimestamp(c.recorded_at)}
        className="font-mono text-mono leading-mono"
      />
    ),
  },
  {
    key: "amount",
    header: "Amount",
    numeric: true,
    sortValue: (c) => -c.credit_deducted_nanos,
    cell: (c) => (
      <span
        className="whitespace-nowrap font-mono tabular-nums text-destructive data-[zero=true]:text-muted-foreground"
        data-zero={c.credit_deducted_nanos === 0 ? "true" : undefined}
      >
        {formatChargeAmount(c.credit_deducted_nanos)}
      </span>
    ),
  },
];

function ChargeIdentityCell({ charge }: { charge: ChargeRow }) {
  const label = chargeAgentLabel(charge);
  return <div className="space-y-xs sm:space-y-0">
    <span className={EYEBROW_CLASS} data-agent-name={label}>{label}</span>
    <div className="space-y-xs text-label text-muted-foreground sm:hidden">
      <p>{displayModelName(charge.model)} · {describeCharge(charge)}</p>
      <Time value={new Date(charge.recorded_at)} label={formatChargeTimestamp(charge.recorded_at)} />
    </div>
  </div>;
}

export default function BillingUsageTab({
  initialCharges,
  initialCursor,
  pageSize,
}: BillingUsageTabProps) {
  // The page lives in the URL and the Server Component above already fetched
  // it, so this component holds no rows, no cache, and no fetch state.
  const feed = useUrlCursorPages(initialCursor, pageSize);
  const charges = initialCharges;

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-3">
      <DataTable
        className="flex min-h-0 flex-1 flex-col"
        columns={COLUMNS}
        rows={charges}
        rowKey={(c) => c.id}
        caption="usage history"
        viewportClassName="min-h-0 flex-1 max-h-none"
        empty={
          <EmptyState
            icon={<ActivityIcon size={28} />}
            title="No charges yet"
            description="Charges appear as setup chats settle and fleets run."
          />
        }
        pagination={{
          kind: PAGINATION_KIND.page,
          page: feed.page,
          pageSize,
          hasNext: feed.hasNext,
          totalLabel: "charges",
          onPageChange: feed.goToPage,
          pageSizeOptions: TABLE_PAGE_SIZE_OPTIONS,
          onPageSizeChange: feed.changePageSize,
          isLoading: feed.isLoading,
        }}
      />
    </div>
  );
}
