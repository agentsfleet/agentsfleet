import { notFound, redirect } from "next/navigation";
import {
  PageHeader,
  PageTitle,
  Section,
} from "@agentsfleet/design-system";
import { auth } from "@clerk/nextjs/server";
import { listWorkspaceEvents } from "@/lib/api/events";
import { resolveActiveWorkspace } from "@/lib/workspace";
import { EventsList } from "@/components/domain/EventsList";

export const dynamic = "force-dynamic";

export default async function EventsPage() {
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) redirect("/sign-in");

  const workspace = await resolveActiveWorkspace(token);
  if (!workspace) notFound();

  const page = await listWorkspaceEvents(workspace.id, token, { limit: 50 }).catch(
    () => ({ items: [], next_cursor: null }),
  );

  return (
    <div>
      <PageHeader>
        <PageTitle>Events</PageTitle>
      </PageHeader>

      <Section asChild>
        <section aria-label="Workspace events">
          <EventsList
            scope={{ kind: "workspace", workspaceId: workspace.id }}
            initial={page}
          />
        </section>
      </Section>
    </div>
  );
}
