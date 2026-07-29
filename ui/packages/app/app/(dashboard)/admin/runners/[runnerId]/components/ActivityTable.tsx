"use client";

import { useMemo } from "react";
import { ActivityIcon } from "lucide-react";
import {
  DataTable,
  type DataTableColumn,
  EmptyState,
  PAGINATION_KIND,
  Time,
} from "@agentsfleet/design-system";
import {
  RUNNER_LIFECYCLE_EVENT_TYPES,
  SANDBOX_TIER_LABELS,
  type RunnerEventItem,
  type RunnerEventsResponse,
  type RunnerEventType,
  type SandboxTier,
} from "@/lib/api/runners";
import { TABLE_PAGE_SIZE_OPTIONS } from "@/lib/pagination/cursor-trail";
import { useUrlCursorPages } from "@/lib/pagination/use-url-cursor-pages";
import {
  ACTIVITY_EMPTY_DESCRIPTION,
  ACTIVITY_EMPTY_TITLE,
  ACTIVITY_TABLE_LABEL,
} from "./runner-copy";

// Lifecycle records only, in the same standard table as Leases. The server is
// asked for the lifecycle type set (RUNNER_LIFECYCLE_EVENT_TYPES, passed by
// the Server Component), so lease_acquired / lease_released never reach this
// feed — the Leases table already states each of them once with an outcome.

// What each lifecycle record SAYS, in operator words.
const EVENT_HEADLINES: Record<RunnerEventType, string> = {
  runner_registered: "registered",
  runner_online: "came online",
  runner_offline: "went offline",
  lease_acquired: "acquired a lease",
  lease_released: "released a lease",
  runner_cordoned: "cordoned",
  runner_draining: "draining",
  runner_drained: "drained",
  runner_revoked: "revoked",
};

// Metadata keys the daemon writes (fleet/runner_events.zig META_*), spelled
// identically here so a renamed key breaks a test rather than a rendering.
const META_FROM_ADMIN_STATE = "from_admin_state";
const META_TO_ADMIN_STATE = "to_admin_state";
const META_HOST_ID = "host_id";
const META_SANDBOX_TIER = "sandbox_tier";
const STATE_TRANSITION_SEPARATOR = " → ";
const DETAIL_SEPARATOR = " · ";

function metaString(metadata: unknown, key: string): string | null {
  if (typeof metadata !== "object" || metadata === null) return null;
  const value = (metadata as Record<string, unknown>)[key];
  return typeof value === "string" ? value : null;
}

// Metadata carries whatever tier tag the daemon recorded at registration; a
// tag minted after this build falls back to its raw spelling rather than
// rendering nothing.
function tierLabelFor(tier: string): string {
  return Object.hasOwn(SANDBOX_TIER_LABELS, tier) ? SANDBOX_TIER_LABELS[tier as SandboxTier] : tier;
}

// The detail column: a state change renders its from- and to-state from the
// event's own metadata; registration renders the host identity and isolation
// tier it recorded, with the real date in the When column.
function detailFor(item: RunnerEventItem): string {
  const from = metaString(item.metadata, META_FROM_ADMIN_STATE);
  const to = metaString(item.metadata, META_TO_ADMIN_STATE);
  if (from !== null && to !== null) return `${from}${STATE_TRANSITION_SEPARATOR}${to}`;
  const host = metaString(item.metadata, META_HOST_ID);
  const tier = metaString(item.metadata, META_SANDBOX_TIER);
  if (host !== null || tier !== null) {
    const tier_label = tier !== null ? tierLabelFor(tier) : null;
    return [host, tier_label].filter((part) => part !== null).join(DETAIL_SEPARATOR);
  }
  return "";
}

const LIFECYCLE_SET: ReadonlySet<RunnerEventType> = new Set(RUNNER_LIFECYCLE_EVENT_TYPES);

export function ActivityTable({ initial, pageSize }: { initial: RunnerEventsResponse; pageSize: number }) {
  const feed = useUrlCursorPages(initial.next_cursor, pageSize);

  // The server is asked for the lifecycle set, and the feed holds the same
  // line locally: a work record reaching this table renders nothing rather
  // than reintroducing the doubled count the filter exists to remove.
  const rows = useMemo(
    () => initial.items.filter((item) => LIFECYCLE_SET.has(item.event_type)),
    [initial.items],
  );

  const columns = useMemo<DataTableColumn<RunnerEventItem>[]>(
    () => [
      {
        key: "when",
        header: "When",
        sortValue: (item) => item.occurred_at,
        cell: (item) => (
          <Time
            value={new Date(item.occurred_at)}
            format="relative"
            className="font-mono text-xs text-muted-foreground tabular-nums"
          />
        ),
      },
      {
        key: "what",
        header: "What",
        cell: (item) => <span className="font-mono text-sm">{EVENT_HEADLINES[item.event_type]}</span>,
      },
      {
        key: "detail",
        header: "Detail",
        cell: (item) => <span className="truncate text-muted-foreground">{detailFor(item)}</span>,
      },
    ],
    [],
  );

  return (
    <DataTable
      caption={ACTIVITY_TABLE_LABEL}
      columns={columns}
      rows={rows}
      rowKey={(item) => item.id}
      pagination={{
        kind: PAGINATION_KIND.page,
        page: feed.page,
        pageSize,
        hasNext: feed.hasNext,
        total: initial.total ?? undefined,
        totalLabel: "records",
        onPageChange: feed.goToPage,
        pageSizeOptions: TABLE_PAGE_SIZE_OPTIONS,
        onPageSizeChange: feed.changePageSize,
        isLoading: feed.isLoading,
      }}
      empty={
        <EmptyState
          icon={<ActivityIcon size={28} />}
          title={ACTIVITY_EMPTY_TITLE}
          description={ACTIVITY_EMPTY_DESCRIPTION}
        />
      }
    />
  );
}
