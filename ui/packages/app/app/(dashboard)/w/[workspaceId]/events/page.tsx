import { Suspense } from "react";
import {
  PageHeader,
  PageLayout,
  PageTitle,
  Section,
  SectionHeader,
  Skeleton,
} from "@agentsfleet/design-system";
import { credential, requireCredential } from "@/lib/auth/credential";
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
  await requireCredential();

  // Header streams first; the stream loads inside EventsData under Suspense.
  return (
    // `h-full overflow-hidden`, not `min-h-full`: the table below fills its
    // parent with `flex-1`, and `flex-1` needs a parent of DEFINITE height to
    // fill. A minimum is not a definite height, so the table collapsed to its
    // content — five events left the wall floating in the top third of the
    // screen with its pagination bar tucked under them, while Secrets and the
    // Fleet library (both `h-full`) spanned correctly. Same shell, same result.
    <PageLayout fullHeight className="h-full overflow-hidden">
      <PageHeader description={EVENTS_DESCRIPTION}>
        <PageTitle>Events</PageTitle>
      </PageHeader>

      <Suspense fallback={<Skeleton className="min-h-0 flex-1 rounded-lg" />}>
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
  const token = await credential();
  if (!token) return null;

  // The cursor comes from the URL, so this page is fetched on the server for
  // every page turn — no rows travel through a Server Action into a client
  // cache, and a reload lands on the same page.
  const page = await listWorkspaceEvents(workspaceId, token, {
    limit: pageSize,
    ...(cursor ? { cursor } : {}),
  });

  return (
    <Section aria-label="Workspace events" className="flex min-h-0 flex-1 flex-col gap-xl">
      <SectionHeader as="p">Manage events</SectionHeader>
      <EventsList initial={page} pageSize={pageSize} />
    </Section>
  );
}
