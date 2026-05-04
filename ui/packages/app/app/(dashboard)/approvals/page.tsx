import { notFound, redirect } from "next/navigation";
import { PageHeader, PageTitle, Section, SectionLabel } from "@usezombie/design-system";

import { getServerToken } from "@/lib/auth/server";
import { resolveActiveWorkspace } from "@/lib/workspace";
import { listApprovals } from "@/lib/api/approvals";
import ApprovalsList from "./components/ApprovalsList";

export const dynamic = "force-dynamic";

export default async function ApprovalsPage() {
  const token = await getServerToken();
  if (!token) redirect("/sign-in");
  const workspace = await resolveActiveWorkspace(token);
  if (!workspace) notFound();

  const initial = await listApprovals(workspace.id, token, { limit: 50 }).catch(() => ({
    items: [],
    next_cursor: null,
  }));

  return (
    <div>
      <PageHeader>
        <PageTitle>Approvals</PageTitle>
      </PageHeader>

      <Section asChild>
        <section aria-label="Pending approval gates">
          <SectionLabel>Pending</SectionLabel>
          <ApprovalsList
            workspaceId={workspace.id}
            initialItems={initial.items}
            initialCursor={initial.next_cursor}
          />
        </section>
      </Section>
    </div>
  );
}
