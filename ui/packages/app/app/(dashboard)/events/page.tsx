import { redirect } from "next/navigation";
import { notFound } from "next/navigation";
import {
  PageHeader,
  PageTitle,
  Section,
  SectionLabel,
} from "@usezombie/design-system";
import { getServerToken } from "@/lib/auth/server";
import { listWorkspaceEvents } from "@/lib/api/events";
import { resolveActiveWorkspace } from "@/lib/workspace";
import { EventsList } from "@/components/domain/EventsList";

export const dynamic = "force-dynamic";

export default async function EventsPage() {
  const token = await getServerToken();
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
          <SectionLabel>Workspace events</SectionLabel>
          <EventsList
            scope={{ kind: "workspace", workspaceId: workspace.id }}
            initial={page}
          />
        </section>
      </Section>
    </div>
  );
}
