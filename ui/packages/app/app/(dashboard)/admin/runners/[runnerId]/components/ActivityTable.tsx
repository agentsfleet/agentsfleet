"use client";

import { TIME_LABEL } from "@/lib/time-labels";
import { useMemo } from "react";
import { ActivityIcon } from "lucide-react";
import {
  DataTable,
  type DataTableColumn,
  EmptyState,
  PAGINATION_KIND,
  Time,
} from "@agentsfleet/design-system";
import { type RunnerEventItem, type RunnerEventsResponse } from "@/lib/api/runners";
import { RUNNER_LIFECYCLE_EVENT_TYPES, type RunnerEventType, type RunnerLifecycleEventType } from "@/lib/api/runners-types";
import { runnerActivity } from "./runner-activity";
import { TABLE_PAGE_SIZE_OPTIONS } from "@/lib/pagination/cursor-trail";
import { useUrlCursorPages } from "@/lib/pagination/use-url-cursor-pages";
import {
  ACTIVITY_EMPTY_DESCRIPTION,
  ACTIVITY_EMPTY_TITLE,
  ACTIVITY_TABLE_LABEL,
} from "./runner-copy";

type LifecycleEventItem = RunnerEventItem & {event_type: RunnerLifecycleEventType};
type ActivityRow = LifecycleEventItem & ReturnType<typeof runnerActivity>;

const LIFECYCLE_SET: ReadonlySet<RunnerEventType> = new Set(RUNNER_LIFECYCLE_EVENT_TYPES);

// The server is asked for the lifecycle set, and the feed holds the same
// line locally: a work record reaching this table renders nothing rather
// than reintroducing the doubled count the filter exists to remove.
function isLifecycleRecord(item: RunnerEventItem): item is LifecycleEventItem {
  return LIFECYCLE_SET.has(item.event_type);
}

const COLUMNS: DataTableColumn<ActivityRow>[] = [
  {
    key: "activity", primary: true,
    header: "Activity",
    cell: (item) => <span className="text-sm">{item.activity}</span>,
  },
  {
    key: "details",
    header: "Details",
    cell: (item) => <span className="text-muted-foreground">{item.details}</span>,
  },
  {
    key: "time",
    header: TIME_LABEL.occurred,
    sortValue: (item) => Date.parse(item.occurred_at),
    cell: (item) => <Time value={new Date(item.occurred_at)} format="relative"
      className="font-sans text-body-sm leading-body-sm text-muted-foreground tabular-nums" />,
  },
];

export function ActivityTable({ initial, pageSize }: { initial: RunnerEventsResponse; pageSize: number }) {
  const feed = useUrlCursorPages(initial.next_cursor, pageSize);

  const rows = useMemo(() => initial.items.filter(isLifecycleRecord)
    .map((item) => ({...item, ...runnerActivity(item)})), [initial.items]);

  // The relative `Time` cells below need tooltip context. It comes from the
  // root layout's single provider, not from here — see `app/layout.tsx`.
  return (
    <div className="flex min-h-0 flex-1 flex-col">
    <DataTable
      caption={ACTIVITY_TABLE_LABEL}
      columns={COLUMNS}
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
    </div>
  );
}
