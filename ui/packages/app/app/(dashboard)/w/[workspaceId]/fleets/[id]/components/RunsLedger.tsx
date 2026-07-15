import { Alert, Badge, type BadgeVariant, Card, EmptyState, Time } from "@agentsfleet/design-system";
import { ReceiptIcon } from "lucide-react";
import type { EventRow, EventStatusValue } from "@/lib/api/events";
import { formatMs } from "@/lib/utils";
import { formatDollars } from "@/app/(dashboard)/settings/billing/lib/charges";
import {
  LEDGER_COST_UNKNOWN,
  LEDGER_EMPTY_DESCRIPTION,
  LEDGER_EMPTY_TITLE,
  LEDGER_PANEL_TITLE,
  ROLLUP_FAILED_LABEL,
  ROLLUP_LIFETIME_LABEL,
  ROLLUP_SPEND_LABEL,
  ROLLUP_TOKENS_LABEL,
  ROLLUP_WAKES_LABEL,
  ROLLUP_WINDOW_LABEL,
  ROLLUP_WINDOW_UNAVAILABLE,
} from "./console-copy";

// The event statuses that count as a failed run in the 7-day rollup. Named so
// the classification is one edit, not scattered string compares (RULE UFS).
const FAILED_EVENT_STATUSES = new Set<EventStatusValue>(["fleet_error", "gate_blocked"]);

const STATUS_VARIANT: Record<string, BadgeVariant> = {
  processed: "green",
  fleet_error: "destructive",
  gate_blocked: "amber",
  received: "cyan",
};

const COUNT_FORMATTER = new Intl.NumberFormat("en-US");

type Rollup = { wakes: number; tokens: number; spendNanos: number; failed: number };

// Client-side 7-day rollup (§6). Every figure sums a server field: spend adds
// `cost_nanos` (a null cost contributes 0 but still counts as a wake — a missing
// telemetry row never vanishes a run), tokens adds `tokens`, failed counts the
// failure statuses. No token→cost arithmetic (Invariant 1).
function computeRollup(events: EventRow[]): Rollup {
  return events.reduce<Rollup>(
    (acc, e) => ({
      wakes: acc.wakes + 1,
      tokens: acc.tokens + (e.tokens ?? 0),
      spendNanos: acc.spendNanos + (e.cost_nanos ?? 0),
      failed: acc.failed + (FAILED_EVENT_STATUSES.has(e.status) ? 1 : 0),
    }),
    { wakes: 0, tokens: 0, spendNanos: 0, failed: 0 },
  );
}

type Props = {
  // The 7-day events window the rollup and list render over; `null` when the
  // window fetch failed — the rollup then degrades to the lifetime figure.
  windowEvents: EventRow[] | null;
  // Lifetime spend, server-truth `budget_used_nanos` from §1 — shown verbatim.
  lifetimeBudgetNanos: number;
};

export default function RunsLedger({ windowEvents, lifetimeBudgetNanos }: Props) {
  const rollup = windowEvents === null ? null : computeRollup(windowEvents);
  const rows = windowEvents === null ? [] : [...windowEvents].sort((a, b) => b.created_at - a.created_at);
  return (
    <Card className="flex flex-col gap-md bg-card p-4" aria-label={LEDGER_PANEL_TITLE}>
      <span className="font-mono text-sm font-medium text-foreground">{LEDGER_PANEL_TITLE}</span>

      <div className="flex flex-col gap-xs" aria-label={ROLLUP_WINDOW_LABEL}>
        <span className="font-mono text-eyebrow uppercase text-muted-foreground">{ROLLUP_WINDOW_LABEL}</span>
        {rollup === null ? (
          <Alert variant="warning">{ROLLUP_WINDOW_UNAVAILABLE}</Alert>
        ) : (
          <div className="grid grid-cols-2 gap-xs sm:grid-cols-4">
            <Stat label={ROLLUP_WAKES_LABEL} value={COUNT_FORMATTER.format(rollup.wakes)} />
            <Stat label={ROLLUP_TOKENS_LABEL} value={COUNT_FORMATTER.format(rollup.tokens)} />
            <Stat label={ROLLUP_SPEND_LABEL} value={formatDollars(rollup.spendNanos)} />
            <Stat label={ROLLUP_FAILED_LABEL} value={COUNT_FORMATTER.format(rollup.failed)} />
          </div>
        )}
        <Stat label={ROLLUP_LIFETIME_LABEL} value={formatDollars(lifetimeBudgetNanos)} />
      </div>

      {windowEvents === null ? null : rows.length === 0 ? (
        <EmptyState icon={<ReceiptIcon size={28} />} title={LEDGER_EMPTY_TITLE} description={LEDGER_EMPTY_DESCRIPTION} />
      ) : (
        <ul className="flex list-none flex-col gap-2 pl-0">
          {rows.map((row) => (
            <li key={`${row.fleet_id}:${row.event_id}`}>
              <LedgerRow row={row} />
            </li>
          ))}
        </ul>
      )}
    </Card>
  );
}

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex flex-col gap-0">
      <span className="font-mono text-eyebrow uppercase text-muted-foreground">{label}</span>
      <span className="font-mono text-sm tabular-nums text-foreground">{value}</span>
    </div>
  );
}

function LedgerRow({ row }: { row: EventRow }) {
  const variant = STATUS_VARIANT[row.status] ?? "default";
  return (
    <Card className="p-3">
      <div className="flex flex-wrap items-baseline gap-md">
        <Badge variant={variant}>{row.status}</Badge>
        <span className="text-sm text-foreground">{row.actor}</span>
        <span className="ml-auto font-mono text-sm tabular-nums text-foreground" data-testid="ledger-cost">
          {row.cost_nanos === null ? LEDGER_COST_UNKNOWN : formatDollars(row.cost_nanos)}
        </span>
      </div>
      <div className="mt-xs flex flex-wrap items-center gap-md font-mono text-xs tabular-nums text-muted-foreground">
        {row.tokens !== null ? <span>{COUNT_FORMATTER.format(row.tokens)} tok</span> : null}
        {row.wall_ms !== null ? <span>{formatMs(row.wall_ms)}</span> : null}
        <Time value={new Date(row.created_at)} format="relative" tooltip={false} className="ml-auto" />
      </div>
    </Card>
  );
}
