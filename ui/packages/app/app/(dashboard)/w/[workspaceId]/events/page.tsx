import { Suspense } from "react";
import { redirect } from "next/navigation";
import {
  PageHeader,
  PageLayout,
  PageTitle,
  SectionHeader,
  Skeleton,
} from "@agentsfleet/design-system";
import { auth } from "@clerk/nextjs/server";
import { listWorkspaceEvents } from "@/lib/api/events";
// The section aria-label below must equal WORKSPACE_EVENTS_LABEL (the events
// table caption) — the parity is pinned by the events page test.
import { EventsList } from "@/components/domain/EventsList";
import {
  CURSOR_PAGE_SIZE_PARAM,
  CURSOR_TRAIL_PARAM,
  DEFAULT_TABLE_PAGE_SIZE,
  PAGE_SIZE_PARAM,
  cursorForTrail,
  cursorTrailFrom,
  pageSizeFrom,
} from "@/lib/pagination/cursor-trail";

export const dynamic = "force-dynamic";

const EVENTS_DESCRIPTION = "Every action your fleets take, as it happens.";

export default async function EventsPage({
  params,
  searchParams,
}: {
  params: Promise<{ workspaceId: string }>;
  searchParams?: Promise<Record<string, string | string[] | undefined>>;
}) {
  const { workspaceId } = await params;
  const query = searchParams ? await searchParams : {};
  const pageSize = pageSizeFrom(query[PAGE_SIZE_PARAM]);
  const cursor = cursorForTrail(
    cursorTrailFrom(
      query[CURSOR_TRAIL_PARAM],
      pageSize,
      query[CURSOR_PAGE_SIZE_PARAM],
    ),
  );
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) redirect("/sign-in");

  // Header streams first; the stream loads inside EventsData under Suspense.
  return (
    <PageLayout fullHeight className="min-h-full">
      <PageHeader description={EVENTS_DESCRIPTION}>
        <PageTitle>Events</PageTitle>
      </PageHeader>

      <Suspense fallback={<Skeleton className="h-48 rounded-lg" />}>
        {/* Keyed by cursor so a page turn re-suspends and shows the
            skeleton, rather than holding the previous page's rows. */}
        <EventsData
          key={`${cursor ?? ""}:${pageSize}`}
          workspaceId={workspaceId}
          cursor={cursor}
          pageSize={pageSize}
        />
      </Suspense>
    </PageLayout>
  );
}

// Async data region: fetches the workspace event stream (workspace from the
// URL). Exported for isolated rendering.
export async function EventsData({
  workspaceId,
  cursor,
  pageSize = DEFAULT_TABLE_PAGE_SIZE,
}: {
  workspaceId: string;
  cursor?: string | null;
  pageSize?: number;
}) {
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) return null;

  // The cursor comes from the URL, so this page is fetched on the server for
  // every page turn — no rows travel through a Server Action into a client
  // cache, and a reload lands on the same page.
  const page = await listWorkspaceEvents(workspaceId, token, {
    limit: pageSize,
    ...(cursor ? { cursor } : {}),
  }).catch(() => ({ items: [], next_cursor: null }));

  return (
    <div
      aria-label="Workspace events"
      className="flex min-h-0 flex-1 flex-col gap-xl"
    >
      <SectionHeader>Manage events</SectionHeader>
      <EventsList initial={page} pageSize={pageSize} />
    </div>
  );
}
