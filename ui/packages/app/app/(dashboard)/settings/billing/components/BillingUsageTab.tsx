"use client";

import { useCallback } from "react";
import { ActivityIcon } from "lucide-react";
import {
  Alert,
  Badge,
  DataTable,
  EmptyState,
  PAGINATION_KIND,
  Time,
  type DataTableColumn,
} from "@agentsfleet/design-system";
import { listTenantBillingChargesAction } from "../actions";
import { PROVIDER_MODE } from "@/lib/types";
import {
  describeCharge,
  formatChargeTimestamp,
  formatDollars,
  type ChargeRow,
} from "../lib/charges";
import { presentErrorString } from "@/lib/errors";
import { useCursorPages } from "@/lib/pagination/use-cursor-pages";

export type BillingUsageTabProps = {
  initialCharges: ChargeRow[];
  initialCursor: string | null;
};

/**
 * Read-only Usage ledger — a terminal-native charge history (date · amount ·
 * type · description), newest-first, paged like every other table.
 * Each row is one raw telemetry charge (receive = gate-pass, stage = run); the
 * model + token detail rides the description column. Charges are deductions, so
 * amounts render negative. Pages are fetched via `listTenantBillingChargesAction`,
 * a Server Action that mints the session token via `auth().getToken()`.
 */
const PAGE_SIZE = 25;

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
  const fetchPage = useCallback(
    (cursor: string) => listTenantBillingChargesAction({ limit: PAGE_SIZE, cursor }),
    [],
  );
  // The ledger pages like every other table instead of appending forever. The
  // old control sat under a list that grew on every press, so reaching it
  // meant scrolling past everything already read.
  const feed = useCursorPages<ChargeRow>(
    { items: initialCharges, next_cursor: initialCursor },
    fetchPage,
    (result) => presentErrorString({
      errorCode: result.errorCode,
      message: result.error,
      action: "load usage events",
    }),
  );
  const { items: charges, error } = feed;

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
          pageSize: PAGE_SIZE,
          hasNext: feed.hasNext,
          totalLabel: "charges",
          onPageChange: feed.goToPage,
          isLoading: feed.isLoading,
        }}
      />
      {error ? <Alert variant="destructive">{error}</Alert> : null}
    </div>
  );
}
