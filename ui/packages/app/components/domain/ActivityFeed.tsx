import * as React from "react";
import { cn } from "@usezombie/design-system/utils";
import { EmptyState } from "@usezombie/design-system";

export interface ActivityEvent {
  id: string;
  zombie_id: string;
  workspace_id: string;
  event_type: string;
  detail: string;
  /** epoch milliseconds */
  created_at: number;
  /** optional display name joined from core.zombies. Falls back to zombie_id prefix. */
  zombie_name?: string;
}

export interface ActivityFeedProps {
  events: ActivityEvent[];
  className?: string;
  /** Narrow title for the <section>'s aria-label and <h2>. Omit to hide the header. */
  title?: string;
  /** Shown when events.length === 0. Default empty-state with a helpful hint. */
  empty?: React.ReactNode;
  /** Render each row's zombie label; default = zombie_name ?? short id. */
  formatZombie?: (e: ActivityEvent) => string;
}

function defaultZombieLabel(e: ActivityEvent): string {
  return e.zombie_name ?? e.zombie_id.slice(-6);
}

function formatClockTime(epoch_ms: number): string {
  // Locale-aware HH:MM (24h). We avoid pulling in a formatter library; the
  // raw epoch is surfaced via <time dateTime> for screen readers.
  const d = new Date(epoch_ms);
  const hh = String(d.getHours()).padStart(2, "0");
  const mm = String(d.getMinutes()).padStart(2, "0");
  return `${hh}:${mm}`;
}

export function ActivityFeed({
  events,
  className,
  title,
  empty,
  formatZombie = defaultZombieLabel,
}: ActivityFeedProps) {
  if (events.length === 0) {
    return (
      <>
        {empty ?? (
          <EmptyState
            title="No activity yet"
            description="Events will appear here once your zombies start processing triggers."
          />
        )}
      </>
    );
  }

  return (
    <section
      data-slot="activity-feed"
      data-testid="activity-feed"
      aria-label={title ?? "Recent activity"}
      className={cn("flex flex-col", className)}
    >
      {title ? (
        <h2 className="px-3 pb-2 text-sm font-semibold text-foreground">{title}</h2>
      ) : null}
      <ul role="list" className="divide-y divide-border">
        {events.map((e) => (
          <li
            key={e.id}
            className="flex flex-col gap-1 px-3 py-2 text-sm sm:flex-row sm:items-baseline sm:gap-3"
          >
            <time
              dateTime={new Date(e.created_at).toISOString()}
              className="shrink-0 font-mono text-xs text-muted-foreground tabular-nums"
            >
              {formatClockTime(e.created_at)}
            </time>
            <span className="shrink-0 font-medium text-foreground">
              {formatZombie(e)}
            </span>
            <span className="shrink-0 font-mono text-xs text-info">
              {e.event_type}
            </span>
            <span className="min-w-0 truncate text-muted-foreground" title={e.detail}>
              {e.detail}
            </span>
          </li>
        ))}
      </ul>
    </section>
  );
}
