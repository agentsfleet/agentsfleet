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
function createEventColumns(onInspect: (row: EventRow) => void): DataTableColumn<EventRow>[] {
  return [
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
      cell: (row) => (row.cost_nanos === null ? VALUE_UNKNOWN : formatDollars(row.cost_nanos)),
    },
    {
      key: "tokens",
      header: "Tokens",
      numeric: true,
      hideOnMobile: true,
      sortValue: (row) => row.tokens ?? NULL_METRIC_SORT_VALUE,
      cell: (row) => (row.tokens === null ? VALUE_UNKNOWN : TOKEN_COUNT_FORMAT.format(row.tokens)),
    },
    {
      key: "duration",
      header: "Duration",
      numeric: true,
      hideOnMobile: true,
      sortValue: (row) => row.wall_ms ?? NULL_METRIC_SORT_VALUE,
      cell: (row) => (row.wall_ms === null ? VALUE_UNKNOWN : formatMs(row.wall_ms)),
    },
  ];
}

export function EventsList({ initial, fleetId }: EventsListProps) {
  const [selected, setSelected] = useState<EventRow | null>(null);
  const columns = useMemo(() => {
    const all = createEventColumns(setSelected);
    return fleetId ? all.filter((column) => column.key !== "fleet") : all;
  }, [fleetId]);

  // The page lives in the URL; the Server Component above already fetched it.
  // Nothing is fetched here, so there is no client cache to fall out of sync
  // and no error state of its own — a failed page is the server's to report.
  const feed = useUrlCursorPages(initial.next_cursor);
  const items = initial.items;

  return (
    <div className="flex flex-col gap-3">
      <DataTable
        caption={fleetId ? FLEET_EVENTS_LABEL : WORKSPACE_EVENTS_LABEL}
        columns={columns}
        rows={items}
        rowKey={(row) => `${row.fleet_id}:${row.event_id}`}
        // A page of rows is a screenful, so the table needs no inner scroll
        // box of its own — the fixed 384px bound only clipped it and left
        // dead space below.
        stickyHeader={false}
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
