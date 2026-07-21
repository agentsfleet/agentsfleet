"use client";

import { useState, useTransition } from "react";
import {
  Alert,
  Badge,
  type BadgeVariant,
  DataTable,
  type DataTableColumn,
  EmptyState,
  Pagination,
  Separator,
  Time,
} from "@agentsfleet/design-system";
import { ActivityIcon } from "lucide-react";
import { listWorkspaceEventsAction } from "@/app/(dashboard)/w/[workspaceId]/events/actions";
import type { EventRow, EventsPage } from "@/lib/api/events";
import { presentErrorString } from "@/lib/errors";
import { formatMs } from "@/lib/utils";

export type EventsListProps = {
  workspaceId: string;
  initial: EventsPage;
};

// One label names the surface everywhere: the table caption here and the
// landmark region in events/page.tsx — never re-spelled.
export const WORKSPACE_EVENTS_LABEL = "Workspace events";

const EVENTS_EMPTY_TITLE = "No events yet";
const EVENTS_EMPTY_DESCRIPTION = "Fleet activity appears here.";
// An unknown figure renders a dash — never a fabricated zero.
const VALUE_UNKNOWN = "—";
// The summary tooltip carries more context than the 160-char preview, but a
// multi-megabyte agent response must not ride into the DOM per row.
const SUMMARY_TITLE_MAX_CHARS = 2_000;

// Map server status → Badge variant. Untracked statuses fall through to
// the default (muted) badge — readable, not opinionated.
const STATUS_VARIANT: Record<string, BadgeVariant> = {
  processed: "green",
  fleet_error: "destructive",
  gate_blocked: "amber",
  received: "cyan",
};

// Friendly labels for the runner's FailureClass tags (src/lib/contract/execution_result.zig).
// A tag this table doesn't cover renders its raw name rather than throwing —
// fails soft if the backend ships a new FailureClass this list hasn't caught up to.
const FAILURE_LABEL: Record<string, string> = {
  startup_posture: "Failed a startup safety check",
  policy_deny: "Blocked by fleet policy",
  timeout_kill: "Timed out",
  oom_kill: "Ran out of memory",
  resource_kill: "Hit a resource limit",
  runner_crash: "The runner crashed",
  transport_loss: "Lost connection to the runner",
  landlock_deny: "Blocked by the sandbox policy",
  lease_expired: "The run's lease expired",
  renewal_terminate: "Stopped by lease renewal policy",
};

function failureLabel(tag: string): string {
  return FAILURE_LABEL[tag] ?? tag;
}

// The workspace event feed in the standard table every sibling data surface
// uses (API keys, secrets, runners, billing). One row per event; the summary
// column carries the response preview or the plain-language failure reason.
const EVENT_COLUMNS: DataTableColumn<EventRow>[] = [
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
  { key: "actor", header: "Actor", cell: (row) => row.actor },
  { key: "type", header: "Type", hideOnMobile: true, cell: (row) => row.event_type },
  { key: "summary", header: "Summary", cell: (row) => <EventSummaryCell row={row} /> },
  {
    key: "tokens",
    header: "Tokens",
    numeric: true,
    hideOnMobile: true,
    cell: (row) => (row.tokens === null ? VALUE_UNKNOWN : row.tokens.toLocaleString()),
  },
  {
    key: "duration",
    header: "Duration",
    numeric: true,
    hideOnMobile: true,
    cell: (row) => (row.wall_ms === null ? VALUE_UNKNOWN : formatMs(row.wall_ms)),
  },
];

export function EventsList({ workspaceId, initial }: EventsListProps) {
  const [items, setItems] = useState<EventRow[]>(initial.items);
  const [cursor, setCursor] = useState<string | null>(initial.next_cursor);
  const [error, setError] = useState<string | null>(null);
  const [pending, startTransition] = useTransition();

  function loadMore(nextCursor: string) {
    setError(null);
    startTransition(async () => {
      const result = await listWorkspaceEventsAction(workspaceId, { cursor: nextCursor });
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
        caption={WORKSPACE_EVENTS_LABEL}
        columns={EVENT_COLUMNS}
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
      tooltip
      label={clockTime(created)}
      tooltipContent={created.toISOString()}
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
        className="block max-w-prose truncate text-muted-foreground"
        title={row.response_text?.slice(0, SUMMARY_TITLE_MAX_CHARS)}
      >
        {preview}
      </span>
    );
  }
  if (row.failure_label) {
    return <span className="text-warning">{failureLabel(row.failure_label)}</span>;
  }
  return null;
}

function previewText(text: string | null): string {
  if (!text) return "";
  const oneline = text.replace(/\s+/g, " ").trim();
  return oneline.length > 160 ? `${oneline.slice(0, 157)}…` : oneline;
}

function shortId(id: string): string {
  return id.length > 12 ? `${id.slice(0, 4)}…${id.slice(-4)}` : id;
}

// Visible HH:MM clock label is intentionally browser-local — operators
// scan the activity feed in their own time zone. The Tooltip surfaces
// the canonical UTC ISO string (`tooltipContent`), so the precise
// instant is always one hover away. Using Intl rather than getHours()
// so locale formatting (24h vs 12h) follows the user's region instead
// of forcing 24h everywhere.
const CLOCK_FORMAT = new Intl.DateTimeFormat(undefined, {
  hour: "2-digit",
  minute: "2-digit",
});

function clockTime(d: Date): string {
  return CLOCK_FORMAT.format(d);
}
