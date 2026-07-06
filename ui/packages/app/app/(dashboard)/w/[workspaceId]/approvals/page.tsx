import { Suspense } from "react";
import { redirect } from "next/navigation";
import { PageHeader, PageTitle, Section, Skeleton } from "@agentsfleet/design-system";

import { auth } from "@clerk/nextjs/server";
import { listApprovals } from "@/lib/api/approvals";
import ApprovalsList from "./components/ApprovalsList";

export const dynamic = "force-dynamic";

const APPROVALS_DESCRIPTION = "Fleet actions that pause for human review.";

export default async function ApprovalsPage({
  params,
}: {
  params: Promise<{ workspaceId: string }>;
}) {
  const { workspaceId } = await params;
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) redirect("/sign-in");

  // Header streams first; the inbox loads inside ApprovalsData under Suspense.
  return (
    <div>
      <PageHeader description={APPROVALS_DESCRIPTION}>
        <PageTitle>Approvals</PageTitle>
      </PageHeader>

      <Suspense fallback={<Skeleton className="h-48 rounded-lg" />}>
        <ApprovalsData workspaceId={workspaceId} />
      </Suspense>
    </div>
  );
}

// Async data region: loads the pending-approval inbox (workspace from the URL).
// Exported so it renders/tests in isolation.
export async function ApprovalsData({ workspaceId }: { workspaceId: string }) {
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) return null;

  const initial = await listApprovals(workspaceId, token, { limit: 50 }).catch(
    () => ({ items: [], next_cursor: null }),
  );

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
