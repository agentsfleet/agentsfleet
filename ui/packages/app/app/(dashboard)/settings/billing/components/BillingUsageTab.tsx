"use client";

import { useState, useTransition } from "react";
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

export type BillingUsageTabProps = {
  initialCharges: ChargeRow[];
  initialCursor: string | null;
};

/**
 * Read-only Usage ledger — a terminal-native charge history (date · amount ·
 * type · description), newest-first, with cursor-based "Load more" pagination.
 * Each row is one raw telemetry charge (receive = gate-pass, stage = run); the
 * model + token detail rides the description column. Charges are deductions, so
 * amounts render negative. Pages are fetched via `listTenantBillingChargesAction`,
 * a Server Action that mints the session token via `auth().getToken()`.
 */
const PAGE_SIZE = 50;

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
  const [charges, setCharges] = useState<ChargeRow[]>(initialCharges);
  const [cursor, setCursor] = useState<string | null>(initialCursor);
  const [error, setError] = useState<string | null>(null);
  const [pending, startTransition] = useTransition();

  // CursorPagination invokes this callback only when a cursor is present.
  function loadMore(cursor: string) {
    setError(null);
    startTransition(async () => {
      const result = await listTenantBillingChargesAction({ limit: PAGE_SIZE, cursor });
      if (!result.ok) {
        setError(
          presentErrorString({
            errorCode: result.errorCode,
            message: result.error,
            action: "load more usage events",
          }),
        );
        return;
      }
      // De-dupe by charge id in case the page boundary repeats a row.
      const seen = new Set(charges.map((c) => c.id));
      const fresh = result.data.items.filter((c) => !seen.has(c.id));
      setCharges([...charges, ...fresh]);
      setCursor(result.data.next_cursor);
    });
  }

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
            title={cursor ? "No charges loaded" : "No charges yet"}
            description={cursor ? "Load more to continue." : "Charges appear once fleets run."}
          />
        )}
        pagination={{ kind: PAGINATION_KIND.cursor, nextCursor: cursor, onNext: loadMore, isLoading: pending }}
      />
      {error ? <Alert variant="destructive">{error}</Alert> : null}
    </div>
  );
}
