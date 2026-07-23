"use client";

import { useMemo, useState } from "react";
import {
  Badge,
  type BadgeVariant,
  Button,
  DataTable,
  type DataTableColumn,
  EmptyState,
  PAGINATION_KIND,
  Time,
} from "@agentsfleet/design-system";
import { ActivityIcon, ChevronRightIcon } from "lucide-react";
import { formatDollars } from "@/app/(dashboard)/settings/billing/lib/charges";
import type { EventRow, EventsPage } from "@/lib/api/events";
import { failureSentenceFor, senderLabelFor } from "@/lib/events/event-summary";
import {
  groupEventRows,
  isZeroMetricOnFailure,
  MIN_ROW_GROUP,
} from "@/lib/events/event-row-grouping";
import { EVENTS_PAGE_SIZE } from "@/lib/pagination/cursor-trail";
import { useUrlCursorPages } from "@/lib/pagination/use-url-cursor-pages";
import { formatMs } from "@/lib/utils";
import { EventDetailsDialog } from "./EventDetailsDialog";

export type EventsListProps = {
  /** The page the Server Component fetched for the cursor in the URL. */
  initial: EventsPage;
  fleetId?: string;
};

// One label names the surface everywhere: the table caption here and the
// landmark region in events/page.tsx — never re-spelled.
export const WORKSPACE_EVENTS_LABEL = "Workspace events";
export const FLEET_EVENTS_LABEL = "Fleet events";

const EVENTS_EMPTY_TITLE = "No events yet";
const EVENTS_EMPTY_DESCRIPTION = "Fleet activity appears here.";
// An unknown figure renders a dash — never a fabricated zero.
const VALUE_UNKNOWN = "—";
const NULL_METRIC_SORT_VALUE = -1;
// The summary tooltip carries more context than the 160-char preview, but a
// multi-megabyte agent response must not ride into the DOM per row.
const SUMMARY_TITLE_MAX_CHARS = 2_000;
const TOKEN_COUNT_FORMAT = new Intl.NumberFormat();
const RUNS_PREFIX = "×";
const RUNS_CLOSED_MARK = "▸";
const RUNS_OPEN_MARK = "▾";

// Map server status → Badge variant. Untracked statuses fall through to
// the default (muted) badge — readable, not opinionated.
const STATUS_VARIANT: Record<string, BadgeVariant> = {
  processed: "green",
  fleet_error: "destructive",
  gate_blocked: "amber",
  received: "cyan",
};

// The workspace event feed in the standard table every sibling data surface
// uses (API keys, secrets, runners, billing). One row per event; the summary
// column carries the response preview or the plain-language failure reason.
function createEventColumns(
  onInspect: (row: EventRow) => void,
  runs: RunsColumn,
): DataTableColumn<EventRow>[] {
  return [
    {
      // Leading column: how many consecutive deliveries this row stands for.
      // Blank for a row that stands only for itself, so the eye catches the
      // repeats rather than a column of "×1".
      key: "runs",
      header: "Runs",
      sortValue: (row) => runs.countFor(row) ?? 0,
      cell: (row) => <RunsCell row={row} runs={runs} />,
    },
    {
      key: "time",
      header: "Time",
      hideOnMobile: true,
      sortValue: (row) => row.created_at,
      cell: (row) => <EventTimeCell row={row} />,
    },
    {
      key: "status",
      header: "Status",
      hideOnMobile: true,
      sortValue: (row) => row.status,
      cell: (row) => <Badge variant={STATUS_VARIANT[row.status] ?? "default"}>{row.status}</Badge>,
    },
    {
      key: "fleet",
      header: "Fleet",
      hideOnMobile: true,
      sortValue: (row) => row.fleet_id,
      cell: (row) => <span className="font-mono text-xs">{shortId(row.fleet_id)}</span>,
    },
    { key: "actor", header: "Actor", sortValue: (row) => senderLabelFor(row.actor), cell: (row) => senderLabelFor(row.actor) },
    {
      key: "details",
      header: "Details",
      cell: (row) => (
        <Button
          type="button"
          variant="ghost"
          size="sm"
          className="min-h-11 sm:min-h-0"
          aria-label={`Inspect event ${row.event_id}`}
          onClick={() => onInspect(row)}
        >
          Inspect
          <ChevronRightIcon size={14} aria-hidden="true" />
        </Button>
      ),
    },
    { key: "type", header: "Type", hideOnMobile: true, sortValue: (row) => row.event_type, cell: (row) => row.event_type },
    { key: "result", header: "Result", sortValue: eventSummaryText, cell: (row) => <EventSummaryCell row={row} /> },
    {
      key: "cost",
      header: "Cost",
      numeric: true,
      hideOnMobile: true,
      sortValue: (row) => row.cost_nanos ?? NULL_METRIC_SORT_VALUE,
      cell: (row) => (
        <DimmedWhenAbsent row={row} value={row.cost_nanos}>
          {row.cost_nanos === null ? VALUE_UNKNOWN : formatDollars(row.cost_nanos)}
        </DimmedWhenAbsent>
      ),
    },
    {
      key: "tokens",
      header: "Tokens",
      numeric: true,
      hideOnMobile: true,
      sortValue: (row) => row.tokens ?? NULL_METRIC_SORT_VALUE,
      cell: (row) => (
        <DimmedWhenAbsent row={row} value={row.tokens}>
          {row.tokens === null ? VALUE_UNKNOWN : TOKEN_COUNT_FORMAT.format(row.tokens)}
        </DimmedWhenAbsent>
      ),
    },
    {
      key: "duration",
      header: "Duration",
      numeric: true,
      hideOnMobile: true,
      sortValue: (row) => row.wall_ms ?? NULL_METRIC_SORT_VALUE,
      cell: (row) => (
        <DimmedWhenAbsent row={row} value={row.wall_ms}>
          {row.wall_ms === null ? VALUE_UNKNOWN : formatMs(row.wall_ms)}
        </DimmedWhenAbsent>
      ),
    },
  ];
}

export function EventsList({ initial, fleetId }: EventsListProps) {
  const [selected, setSelected] = useState<EventRow | null>(null);
  const [opened, setOpened] = useState<ReadonlySet<string>>(() => new Set());

  // The page lives in the URL; the Server Component above already fetched it.
  // Nothing is fetched here, so there is no client cache to fall out of sync
  // and no error state of its own — a failed page is the server's to report.
  const feed = useUrlCursorPages(initial.next_cursor);

  // Runs of the same failure collapse to their first row. Page-local by
  // construction: this only ever sees the rows the server returned.
  const entries = useMemo(() => groupEventRows(initial.items), [initial.items]);
  const items = useMemo(
    () =>
      entries.flatMap((entry) =>
        entry.rows.length < MIN_ROW_GROUP || opened.has(entry.lead.event_id)
          ? entry.rows
          : [entry.lead],
      ),
    [entries, opened],
  );
  const runs = useMemo<RunsColumn>(() => {
    const counts = new Map<string, number>();
    for (const entry of entries) {
      if (entry.rows.length >= MIN_ROW_GROUP) counts.set(entry.lead.event_id, entry.rows.length);
    }
    return {
      countFor: (row) => counts.get(row.event_id) ?? null,
      isOpen: (row) => opened.has(row.event_id),
      toggle: (row) =>
        setOpened((prev) => {
          const next = new Set(prev);
          if (!next.delete(row.event_id)) next.add(row.event_id);
          return next;
        }),
    };
  }, [entries, opened]);

  const columns = useMemo(() => {
    const all = createEventColumns(setSelected, runs);
    return fleetId ? all.filter((column) => column.key !== "fleet") : all;
  }, [fleetId, runs]);

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <DataTable
        className="flex min-h-0 flex-1 flex-col"
        caption={fleetId ? FLEET_EVENTS_LABEL : WORKSPACE_EVENTS_LABEL}
        columns={columns}
        rows={items}
        rowKey={(row) => `${row.fleet_id}:${row.event_id}`}
        viewportClassName="min-h-0 flex-1 max-h-none"
        pagination={{
          kind: PAGINATION_KIND.page,
          page: feed.page,
          pageSize: EVENTS_PAGE_SIZE,
          hasNext: feed.hasNext,
          totalLabel: "events",
          onPageChange: feed.goToPage,
          isLoading: feed.isLoading,
        }}
        empty={
          <EmptyState
            icon={<ActivityIcon size={28} />}
            title={EVENTS_EMPTY_TITLE}
            description={EVENTS_EMPTY_DESCRIPTION}
          />
        }
      />
      <EventDetailsDialog
        row={selected}
        onOpenChange={() => setSelected(null)}
      />
    </div>
  );
}

type RunsColumn = {
  /** How many rows this one stands for, or null when it stands alone. */
  countFor: (row: EventRow) => number | null;
  isOpen: (row: EventRow) => boolean;
  toggle: (row: EventRow) => void;
};

// The count is a control, not a label: it opens to the rows it covers, so an
// operator never has to take "×15" on trust.
function RunsCell({ row, runs }: { row: EventRow; runs: RunsColumn }) {
  const count = runs.countFor(row);
  if (count === null) return null;
  const open = runs.isOpen(row);
  return (
    <Button
      type="button"
      variant="ghost"
      size="sm"
      aria-expanded={open}
      aria-label={`${open ? "Collapse" : "Expand"} ${count} repeated failures`}
      onClick={() => runs.toggle(row)}
    >
      <Badge variant="destructive">{`${RUNS_PREFIX}${count}`}</Badge>
      <span aria-hidden="true">{open ? RUNS_OPEN_MARK : RUNS_CLOSED_MARK}</span>
    </Button>
  );
}

// A failed run's zero is an absence, not a measurement — it reports that
// nothing ran, and rendering it at full weight invites reading it as a real
// figure. A successful run's zero is a genuine result and stays.
function DimmedWhenAbsent({
  row,
  value,
  children,
}: {
  row: EventRow;
  value: number | null;
  children: React.ReactNode;
}) {
  if (!isZeroMetricOnFailure(row, value)) return <>{children}</>;
  return <span className="text-muted-foreground/50">{children}</span>;
}

function EventTimeCell({ row }: { row: EventRow }) {
  const created = new Date(row.created_at);
  if (!isFinite(created.getTime())) return null;
  return (
    <Time
      value={created}
      format="relative"
      className="font-mono text-xs text-muted-foreground tabular-nums"
    />
  );
}

// The one prose cell: a truncated response preview (full text one hover away)
// or, for a failed run, the plain-language reason the operator can act on.
function EventSummaryCell({ row }: { row: EventRow }) {
  const preview = previewText(row.response_text);
  if (preview) {
    return (
      <span
        className="block max-w-prose truncate py-sm text-foreground"
        title={row.response_text?.slice(0, SUMMARY_TITLE_MAX_CHARS)}
      >
        {preview}
      </span>
    );
  }
  if (row.failure_label) {
    return <span className="text-warning">{failureSentenceFor(row.failure_label)}</span>;
  }
  return <span className="text-muted-foreground">No result recorded</span>;
}

function eventSummaryText(row: EventRow): string {
  const preview = previewText(row.response_text);
  if (preview) return preview;
  if (row.failure_label) return failureSentenceFor(row.failure_label);
  return "No result recorded";
}

function previewText(text: string | null): string {
  if (!text) return "";
  const oneline = text.replace(/\s+/g, " ").trim();
  return oneline.length > 160 ? `${oneline.slice(0, 157)}…` : oneline;
}

function shortId(id: string): string {
  return id.length > 12 ? `${id.slice(0, 4)}…${id.slice(-4)}` : id;
}
