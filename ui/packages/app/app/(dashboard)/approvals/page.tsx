import { Suspense } from "react";
import { notFound, redirect } from "next/navigation";
import { PageHeader, PageTitle, Section, Skeleton } from "@agentsfleet/design-system";

import { auth } from "@clerk/nextjs/server";
import { withWorkspaceScope, orFallback } from "@/lib/workspace";
import { listApprovals } from "@/lib/api/approvals";
import ApprovalsList from "./components/ApprovalsList";

export const dynamic = "force-dynamic";

export default async function ApprovalsPage() {
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) redirect("/sign-in");

  // Header streams first; the inbox loads inside ApprovalsData under Suspense.
  return (
    <div>
      <PageHeader>
        <PageTitle>Approvals</PageTitle>
      </PageHeader>

      <Suspense fallback={<Skeleton className="h-48 rounded-lg" />}>
        <ApprovalsData />
      </Suspense>
    </div>
  );
}

// Async data region: resolves the active workspace from the cookie/JWT hint and
// loads the pending-approval inbox. Exported so it renders/tests in isolation.
export async function ApprovalsData() {
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) return null;

  const result = await withWorkspaceScope(token, async (workspaceId) => ({
    workspaceId,
    initial: await listApprovals(workspaceId, token, { limit: 50 }).catch(
      orFallback({ items: [], next_cursor: null }),
    ),
  }));
  if (!result) notFound();
  const { workspaceId, initial } = result;

  return (
    <Section asChild>
      <section aria-label="Pending approval gates">
        <ApprovalsList
          workspaceId={workspaceId}
          initialItems={initial.items}
          initialCursor={initial.next_cursor}
        />
      </section>
    </Section>
  );
}
