"use client";

import { useState, useTransition } from "react";
import {
  Alert,
  Badge,
  type BadgeVariant,
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  CardDescription,
  EmptyState,
  List,
  ListItem,
  Pagination,
  Separator,
  Time,
} from "@usezombie/design-system";
import {
  listWorkspaceEventsAction,
  listZombieEventsAction,
} from "@/app/(dashboard)/events/actions";
import type { EventRow, EventsPage } from "@/lib/api/events";
import { presentErrorString } from "@/lib/errors";

type Scope =
  | { kind: "zombie"; workspaceId: string; zombieId: string }
  | { kind: "workspace"; workspaceId: string };

export type EventsListProps = {
  scope: Scope;
  initial: EventsPage;
  emptyTitle?: string;
  emptyDescription?: string;
};

// Map server status → Badge variant. Untracked statuses fall through to
// the default (muted) badge — readable, not opinionated.
const STATUS_VARIANT: Record<string, BadgeVariant> = {
  processed: "green",
  agent_error: "destructive",
  gate_blocked: "amber",
  received: "cyan",
};

export function EventsList({
  scope,
  initial,
  emptyTitle = "No events yet",
  emptyDescription = "Operator steers, webhooks, and cron triggers will land here once your agents start running.",
}: EventsListProps) {
  const [items, setItems] = useState<EventRow[]>(initial.items);
  const [cursor, setCursor] = useState<string | null>(initial.next_cursor);
  const [error, setError] = useState<string | null>(null);
  const [pending, startTransition] = useTransition();

  function loadMore(nextCursor: string) {
    setError(null);
    startTransition(async () => {
      const result =
        scope.kind === "zombie"
          ? await listZombieEventsAction(scope.workspaceId, scope.zombieId, { cursor: nextCursor })
          : await listWorkspaceEventsAction(scope.workspaceId, { cursor: nextCursor });
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

  if (items.length === 0) {
    return <EmptyState title={emptyTitle} description={emptyDescription} />;
  }

  return (
    <div className="flex flex-col gap-3">
      <List variant="ordered" className="flex flex-col gap-2 list-none pl-0 space-y-0">
        {items.map((row) => (
          <ListItem key={`${row.zombie_id}:${row.event_id}`}>
            <EventCard row={row} showZombieId={scope.kind === "workspace"} />
          </ListItem>
        ))}
      </List>
      {error ? (
        <Alert variant="destructive">{error}</Alert>
      ) : null}
      <Separator />
      <Pagination kind="cursor" nextCursor={cursor} onNext={loadMore} isLoading={pending} />
    </div>
  );
}

function EventCard({ row, showZombieId }: { row: EventRow; showZombieId: boolean }) {
  const created = new Date(row.created_at);
  const ts = isFinite(created.getTime()) ? created.toISOString() : null;
  const variant = STATUS_VARIANT[row.status] ?? "default";
  const preview = previewText(row.response_text);
  const fullText = row.response_text ?? undefined;

  return (
    <Card asChild className="p-4">
      <article aria-label={`Event ${row.event_id} by ${row.actor}, status ${row.status}`}>
        <CardHeader className="flex flex-row flex-wrap items-baseline gap-3 space-y-0 p-0 pb-2">
          <Badge variant={variant}>{row.status}</Badge>
          <CardTitle className="text-sm font-medium text-foreground">{row.actor}</CardTitle>
          <CardDescription className="text-xs text-muted-foreground">
            {row.event_type}
            {showZombieId ? ` · ${shortId(row.zombie_id)}` : ""}
          </CardDescription>
          <div className="ml-auto">
            {ts ? (
              <Time
                value={created}
                tooltip
                label={clockTime(created)}
                tooltipContent={ts}
                className="font-mono text-xs text-muted-foreground tabular-nums"
              />
            ) : null}
          </div>
        </CardHeader>
        <CardContent className="p-0">
          {preview ? (
            <p className="truncate text-sm text-muted-foreground" title={fullText}>
              {preview}
            </p>
          ) : row.failure_label ? (
            <p className="text-sm text-warning">Reason: {row.failure_label}</p>
          ) : null}
        </CardContent>
      </article>
    </Card>
  );
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
// the canonical UTC ISO string (`tooltipContent={ts}`), so the precise
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
