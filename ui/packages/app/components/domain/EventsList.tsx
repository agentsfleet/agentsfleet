"use client";

import { useMemo, useState, useTransition } from "react";
import {
  Alert,
  Badge,
  type BadgeVariant,
  Button,
  DataTable,
  type DataTableColumn,
  EmptyState,
  Pagination,
  Separator,
  Time,
} from "@agentsfleet/design-system";
import { ActivityIcon, ChevronRightIcon } from "lucide-react";
import { listWorkspaceEventsAction } from "@/app/(dashboard)/w/[workspaceId]/events/actions";
import { formatDollars } from "@/app/(dashboard)/settings/billing/lib/charges";
import type { EventRow, EventsPage } from "@/lib/api/events";
import { failureSentenceFor } from "@/lib/events/event-summary";
import { presentErrorString } from "@/lib/errors";
import { formatMs } from "@/lib/utils";
import { EventDetailsDialog } from "./EventDetailsDialog";
import { presentFleetActor } from "./fleetActorPresentation";

export type EventsListProps = {
  workspaceId: string;
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
    { key: "time", header: "Time", cell: (row) => <EventTimeCell row={row} /> },
    {
      key: "status",
      header: "Status",
      cell: (row) => <Badge variant={STATUS_VARIANT[row.status] ?? "default"}>{row.status}</Badge>,
    },
    {
      key: "fleet",
      header: "Fleet",
      hideOnMobile: true,
      cell: (row) => <span className="font-mono text-xs">{shortId(row.fleet_id)}</span>,
    },
    { key: "actor", header: "Actor", cell: (row) => presentFleetActor(row.actor) },
    {
      key: "details",
      header: "Details",
      cell: (row) => (
        <Button
          type="button"
          variant="ghost"
          size="sm"
          aria-label={`Inspect event ${row.event_id}`}
          onClick={() => onInspect(row)}
        >
          Inspect
          <ChevronRightIcon size={14} aria-hidden="true" />
        </Button>
      ),
    },
    { key: "type", header: "Type", hideOnMobile: true, cell: (row) => row.event_type },
    { key: "result", header: "Result", cell: (row) => <EventSummaryCell row={row} /> },
    {
      key: "cost",
      header: "Cost",
      numeric: true,
      hideOnMobile: true,
      cell: (row) => (row.cost_nanos === null ? VALUE_UNKNOWN : formatDollars(row.cost_nanos)),
    },
    {
      key: "tokens",
      header: "Tokens",
      numeric: true,
      hideOnMobile: true,
      cell: (row) => (row.tokens === null ? VALUE_UNKNOWN : TOKEN_COUNT_FORMAT.format(row.tokens)),
    },
    {
      key: "duration",
      header: "Duration",
      numeric: true,
      hideOnMobile: true,
      cell: (row) => (row.wall_ms === null ? VALUE_UNKNOWN : formatMs(row.wall_ms)),
    },
  ];
}

export function EventsList({ workspaceId, initial, fleetId }: EventsListProps) {
  const [items, setItems] = useState<EventRow[]>(initial.items);
  const [cursor, setCursor] = useState<string | null>(initial.next_cursor);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<EventRow | null>(null);
  const [pending, startTransition] = useTransition();
  const columns = useMemo(() => {
    const all = createEventColumns(setSelected);
    return fleetId ? all.filter((column) => column.key !== "fleet") : all;
  }, [fleetId]);

  function loadMore(nextCursor: string) {
    setError(null);
    startTransition(async () => {
      const result = await listWorkspaceEventsAction(workspaceId, {
        cursor: nextCursor,
        ...(fleetId ? { fleet_id: fleetId } : {}),
      });
      if (!result.ok) {
        setError(
          presentErrorString({
            errorCode: result.errorCode,
            message: result.error,
            action: "load more events",
          }),
        );
        return;
      }
      setItems((prev) => [...prev, ...result.data.items]);
      setCursor(result.data.next_cursor);
    });
  }

  return (
    <div className="flex flex-col gap-3">
      <DataTable
        caption={fleetId ? FLEET_EVENTS_LABEL : WORKSPACE_EVENTS_LABEL}
        columns={columns}
        rows={items}
        rowKey={(row) => `${row.fleet_id}:${row.event_id}`}
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
      {error ? <Alert variant="destructive">{error}</Alert> : null}
      {items.length > 0 || cursor ? (
        // Also rendered when a page came back empty but a cursor remains
        // (compaction between pages) — data behind the cursor must never be
        // stranded behind an empty state.
        <>
          <Separator />
          <Pagination kind="cursor" nextCursor={cursor} onNext={loadMore} isLoading={pending} />
        </>
      ) : null}
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

function previewText(text: string | null): string {
  if (!text) return "";
  const oneline = text.replace(/\s+/g, " ").trim();
  return oneline.length > 160 ? `${oneline.slice(0, 157)}…` : oneline;
}

function shortId(id: string): string {
  return id.length > 12 ? `${id.slice(0, 4)}…${id.slice(-4)}` : id;
}
