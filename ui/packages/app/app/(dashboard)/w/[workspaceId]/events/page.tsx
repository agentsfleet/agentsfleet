import { Suspense } from "react";
import { redirect } from "next/navigation";
import {
  PageHeader,
  PageTitle,
  Section,
  Skeleton,
} from "@agentsfleet/design-system";
import { auth } from "@clerk/nextjs/server";
import { listWorkspaceEvents } from "@/lib/api/events";
// The section aria-label below must equal WORKSPACE_EVENTS_LABEL (the events
// table caption) — the parity is pinned by the events page test.
import { EventsList } from "@/components/domain/EventsList";

export const dynamic = "force-dynamic";

const EVENTS_DESCRIPTION = "Every action your fleets take, as it happens.";

export default async function EventsPage({
  params,
}: {
  params: Promise<{ workspaceId: string }>;
}) {
  const { workspaceId } = await params;
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
        <EventsData workspaceId={workspaceId} />
      </Suspense>
    </div>
  );
}

// Async data region: fetches the workspace event stream (workspace from the
// URL). Exported for isolated rendering.
export async function EventsData({ workspaceId }: { workspaceId: string }) {
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) return null;

  const page = await listWorkspaceEvents(workspaceId, token, { limit: 50 }).catch(
    () => ({ items: [], next_cursor: null }),
  );

  return (
    <Section asChild>
      <section aria-label="Workspace events">
        <EventsList workspaceId={workspaceId} initial={page} />
      </section>
    </Section>
  );
}
