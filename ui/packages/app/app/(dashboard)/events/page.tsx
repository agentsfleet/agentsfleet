import { Suspense } from "react";
import { notFound, redirect } from "next/navigation";
import {
  PageHeader,
  PageTitle,
  Section,
  Skeleton,
} from "@agentsfleet/design-system";
import { auth } from "@clerk/nextjs/server";
import { listWorkspaceEvents } from "@/lib/api/events";
import { withWorkspaceScope, orFallback } from "@/lib/workspace";
import { EventsList } from "@/components/domain/EventsList";

export const dynamic = "force-dynamic";

const EVENTS_DESCRIPTION = "Every action your fleets take, as it happens.";

export default async function EventsPage() {
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) redirect("/sign-in");

  // Header streams first; the stream loads inside EventsData under Suspense.
  return (
    <div>
      <PageHeader description={EVENTS_DESCRIPTION}>
        <PageTitle>Events</PageTitle>
      </PageHeader>

      <Suspense fallback={<Skeleton className="h-48 rounded-lg" />}>
        <EventsData />
      </Suspense>
    </div>
  );
}

// Async data region: resolves the active workspace from the cookie/JWT hint and
// fetches the workspace event stream in one pass. `withWorkspaceScope`
// re-resolves + retries once if a stale hint is rejected by the backend. No
// workspace-list round-trip on the hot path. Exported for isolated rendering.
export async function EventsData() {
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) return null;

  const result = await withWorkspaceScope(token, async (workspaceId) => ({
    workspaceId,
    page: await listWorkspaceEvents(workspaceId, token, { limit: 50 }).catch(
      orFallback({ items: [], next_cursor: null }),
    ),
  }));
  if (!result) notFound();
  const { workspaceId, page } = result;

  return (
    <Section asChild>
      <section aria-label="Workspace events">
        <EventsList
          scope={{ kind: "workspace", workspaceId }}
          initial={page}
        />
      </section>
    </Section>
  );
}
