"use client";

import { TIME_LABEL } from "@/lib/time-labels";
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
  chargeUsage,
  formatChargeTimestamp,
  formatChargeAmount,
  type ChargeRow,
} from "../lib/charges";
import { TABLE_PAGE_SIZE_OPTIONS } from "@/lib/pagination/cursor-trail";
import { useUrlCursorPages } from "@/lib/pagination/use-url-cursor-pages";
import { compareNanos } from "@/lib/money";

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
    sortValue: (c) => displayModelName(c.model, c.model_display_name),
    cell: (c) => (
      <span>{displayModelName(c.model, c.model_display_name)}</span>
    ),
  },
  {
    key: "activity",
    header: "Usage",
    hideOnMobile: true,
    sortValue: (c) => describeCharge(c),
    cell: (c) => <ChargeUsageCell charge={c} />,
  },
  {
    key: "created",
    header: TIME_LABEL.occurred,
    hideOnMobile: true,
    sortValue: (c) => Date.parse(c.recorded_at),
    cell: (c) => (
      <Time
        value={c.recorded_at}
        format="relative"
        tooltipContent={formatChargeTimestamp(c.recorded_at)}
      />
    ),
  },
  {
    key: "amount",
    header: "Amount",
    numeric: true,
    sortValue: (c) => c.credit_deducted_nanos,
    compare: (a, b) => compareNanos(b.credit_deducted_nanos, a.credit_deducted_nanos),
    cell: (c) => (
      <span
        className="whitespace-nowrap font-mono tabular-nums text-destructive data-[zero=true]:text-muted-foreground"
        data-zero={c.credit_deducted_nanos === "0" ? "true" : undefined}
      >
        {formatChargeAmount(c.credit_deducted_nanos)}
      </span>
    ),
  },
];

function ChargeIdentityCell({ charge }: { charge: ChargeRow }) {
  const label = chargeAgentLabel(charge);
  return <div className="space-y-xs sm:space-y-0">
    <span className="text-body-sm font-medium" data-agent-name={label}>{label}</span>
    <div className="space-y-xs text-label text-muted-foreground sm:hidden">
      <p>{displayModelName(charge.model, charge.model_display_name)} · {describeCharge(charge)}</p>
      <Time value={charge.recorded_at} format="relative" tooltipContent={formatChargeTimestamp(charge.recorded_at)} />
    </div>
  </div>;
}

function ChargeUsageCell({charge}: {charge: ChargeRow}) {
  const usage = chargeUsage(charge);
  return <div><p>{usage.label}</p>{usage.detail ? <p className="text-label text-muted-foreground">{usage.detail}</p> : null}</div>;
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
