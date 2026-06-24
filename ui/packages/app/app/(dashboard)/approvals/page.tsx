import { notFound, redirect } from "next/navigation";
import { PageHeader, PageTitle, Section } from "@agentsfleet/design-system";

import { auth } from "@clerk/nextjs/server";
import { withWorkspaceScope, orFallback } from "@/lib/workspace";
import { listApprovals } from "@/lib/api/approvals";
import ApprovalsList from "./components/ApprovalsList";

export const dynamic = "force-dynamic";

export default async function ApprovalsPage() {
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) redirect("/sign-in");

  const result = await withWorkspaceScope(token, async (workspaceId) => ({
    workspaceId,
    initial: await listApprovals(workspaceId, token, { limit: 50 }).catch(
      orFallback({ items: [], next_cursor: null }),
    ),
  }));
  if (!result) notFound();
  const { workspaceId, initial } = result;

  return (
    <div>
      <PageHeader>
        <PageTitle>Approvals</PageTitle>
      </PageHeader>

      <Section asChild>
        <section aria-label="Pending approval gates">
          <ApprovalsList
            workspaceId={workspaceId}
            initialItems={initial.items}
            initialCursor={initial.next_cursor}
          />
        </section>
      </Section>
    </div>
  );
}
