"use client";

import { useMemo } from "react";
import { ActivityIcon } from "lucide-react";
import {
  DataTable,
  type DataTableColumn,
  EmptyState,
  formatTimeAbsolute,
  PAGINATION_KIND,
  Time,
} from "@agentsfleet/design-system";
import {
  RUNNER_LAST_SEEN_NEVER,
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
// the Server Component), so lease work records never reach this feed — the
// Leases table already states each of them once with an outcome.

type LifecycleEventType = (typeof RUNNER_LIFECYCLE_EVENT_TYPES)[number];
type LifecycleEventItem = RunnerEventItem & { event_type: LifecycleEventType };

// What each lifecycle record SAYS, in operator words. Keyed on the lifecycle
// subset, so a lease work tag cannot be given a headline here at all.
const EVENT_HEADLINES: Record<LifecycleEventType, string> = {
  runner_registered: "registered",
  runner_online: "came online",
  runner_offline: "went offline",
  runner_cordoned: "cordoned",
  runner_draining: "draining",
  runner_drained: "drained",
  runner_revoked: "revoked",
  runner_policy_assigned: "policy assigned",
};

// Metadata keys the daemon writes (fleet/runner_events.zig META_*), spelled
// identically here so a renamed key breaks a test rather than a rendering.
const META_FROM_ADMIN_STATE = "from_admin_state";
const META_TO_ADMIN_STATE = "to_admin_state";
const META_HOST_ID = "host_id";
const META_SANDBOX_TIER = "sandbox_tier";
const META_LAST_SEEN_AT = "last_seen_at";
const STATE_TRANSITION_SEPARATOR = " → ";
const DETAIL_SEPARATOR = " · ";
const LAST_CONTACT_PREFIX = "last contact ";
const FIRST_CONTACT_LABEL = "first contact";

function metaString(metadata: unknown, key: string): string | null {
  if (typeof metadata !== "object" || metadata === null) return null;
  const value = (metadata as Record<string, unknown>)[key];
  return typeof value === "string" ? value : null;
}

// `last_seen_at` arrives as a JSON NUMBER — both writers build it with
// `jsonb_build_object($key, <bigint>)` (runner/sql.zig HEARTBEAT_WITH_
// TRANSITION_EVENT, fleet/sql.zig INSERT_OFFLINE_EVENT). Reading it through
// metaString returned null for every online/offline record, which is why the
// Detail cell rendered empty for the two most common event types.
function metaNumber(metadata: unknown, key: string): number | null {
  if (typeof metadata !== "object" || metadata === null) return null;
  const value = (metadata as Record<string, unknown>)[key];
  return typeof value === "number" ? value : null;
}

// Metadata carries whatever tier tag the daemon recorded at registration; a
// tag minted after this build falls back to its raw spelling rather than
// rendering nothing.
function tierLabelFor(tier: string): string {
  return Object.hasOwn(SANDBOX_TIER_LABELS, tier) ? SANDBOX_TIER_LABELS[tier as SandboxTier] : tier;
}

// The detail column: a state change renders its from- and to-state from the
// event's own metadata; registration renders the host identity and isolation
// tier it recorded, with the real date in the Time column.
//
// Online and offline records carry `last_seen_at` — the last contact BEFORE the
// transition — and it is the only honest answer to "when did this actually
// happen?". An offline record's `occurred_at` is when the sweeper NOTICED, one
// RUNNER_OFFLINE_AFTER_MS (three lease TTLs) after the runner really went dark,
// so the Time column reads late by construction. An online record's is the gap
// the runner was away. Rendering it absolute rather than relative keeps it from
// reading as a second, contradictory version of the Time column.
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
  const last_seen_at = metaNumber(item.metadata, META_LAST_SEEN_AT);
  if (last_seen_at !== null) {
    // The sentinel is a real state, not missing data: a runner minted but never
    // heard from carries it until its first heartbeat, and "last contact
    // 1 Jan 1970" would be a lie dressed as precision.
    return last_seen_at === RUNNER_LAST_SEEN_NEVER
      ? FIRST_CONTACT_LABEL
      : `${LAST_CONTACT_PREFIX}${formatTimeAbsolute(new Date(last_seen_at))}`;
  }
  return "";
}

const LIFECYCLE_SET: ReadonlySet<RunnerEventType> = new Set(RUNNER_LIFECYCLE_EVENT_TYPES);

// The server is asked for the lifecycle set, and the feed holds the same
// line locally: a work record reaching this table renders nothing rather
// than reintroducing the doubled count the filter exists to remove.
function isLifecycleRecord(item: RunnerEventItem): item is LifecycleEventItem {
  return LIFECYCLE_SET.has(item.event_type);
}

export function ActivityTable({ initial, pageSize }: { initial: RunnerEventsResponse; pageSize: number }) {
  const feed = useUrlCursorPages(initial.next_cursor, pageSize);

  const rows = useMemo(() => initial.items.filter(isLifecycleRecord), [initial.items]);

  const columns = useMemo<DataTableColumn<LifecycleEventItem>[]>(
    () => [
      {
        key: "time",
        header: "Time",
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

  // The relative `Time` cells below need tooltip context. It comes from the
  // root layout's single provider, not from here — see `app/layout.tsx`.
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
