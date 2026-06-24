import { notFound, redirect } from "next/navigation";
import {
  PageHeader,
  PageTitle,
  Section,
} from "@agentsfleet/design-system";
import { auth } from "@clerk/nextjs/server";
import { listWorkspaceEvents } from "@/lib/api/events";
import { withWorkspaceScope, orFallback } from "@/lib/workspace";
import { EventsList } from "@/components/domain/EventsList";

export const dynamic = "force-dynamic";

export default async function EventsPage() {
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) redirect("/sign-in");

  // Resolve the active workspace from the cookie/JWT hint and fetch in one
  // pass; `withWorkspaceScope` re-resolves + retries once if a stale hint is
  // rejected by the backend. No workspace-list round-trip on the hot path.
  const result = await withWorkspaceScope(token, async (workspaceId) => ({
    workspaceId,
    page: await listWorkspaceEvents(workspaceId, token, { limit: 50 }).catch(
      orFallback({ items: [], next_cursor: null }),
    ),
  }));
  if (!result) notFound();
  const { workspaceId, page } = result;

  return (
    <div>
      <PageHeader>
        <PageTitle>Events</PageTitle>
      </PageHeader>

      <Section asChild>
        <section aria-label="Workspace events">
          <EventsList
            scope={{ kind: "workspace", workspaceId }}
            initial={page}
          />
        </section>
      </Section>
    </div>
  );
}
