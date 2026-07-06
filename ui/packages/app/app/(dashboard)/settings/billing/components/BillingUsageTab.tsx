"use client";

import { useState, useTransition } from "react";
import { ActivityIcon } from "lucide-react";
import {
  Alert,
  Badge,
  Button,
  DataTable,
  EmptyState,
  Spinner,
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
    cell: (c) => <span className="font-mono text-xs">{formatChargeTimestamp(c.recorded_at)}</span>,
  },
  {
    key: "amount",
    header: "Amount",
    numeric: true,
    cell: (c) => (
      <span className="font-mono tabular-nums text-destructive">−{formatDollars(c.credit_deducted_nanos)}</span>
    ),
  },
  {
    key: "type",
    header: "Type",
    cell: (c) => (
      <Badge variant={c.posture === PROVIDER_MODE.self_managed ? "cyan" : "default"}>{c.posture}</Badge>
    ),
  },
  {
    key: "description",
    header: "Description",
    cell: (c) => <span className="text-muted-foreground">{describeCharge(c)}</span>,
  },
];

export default function BillingUsageTab({ initialCharges, initialCursor }: BillingUsageTabProps) {
  const [charges, setCharges] = useState<ChargeRow[]>(initialCharges);
  const [cursor, setCursor] = useState<string | null>(initialCursor);
  const [error, setError] = useState<string | null>(null);
  const [pending, startTransition] = useTransition();

  // `cursor` is passed in (narrowed to a non-null string by the `{cursor ? …}`
  // render guard on the trigger), so no in-function null check is needed.
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

  if (charges.length === 0) {
    return (
      <EmptyState
        icon={<ActivityIcon size={28} />}
        title="No charges yet"
        description="Charges appear once fleets run."
      />
    );
  }

  return (
    <div className="space-y-3">
      <DataTable
        columns={COLUMNS}
        rows={charges}
        rowKey={(c) => c.id}
        caption="usage history"
        stickyHeader
      />
      <div className="flex items-center gap-3 text-xs">
        {cursor ? (
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={() => loadMore(cursor)}
            disabled={pending}
            data-testid="usage-load-more"
          >
            {pending ? <Spinner size="sm" srLabel="Loading" /> : null}
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
