import { Suspense } from "react";
import { redirect } from "next/navigation";
import {
  PageHeader,
  PageLayout,
  PageTitle,
  Section,
  SectionHeader,
  Skeleton,
} from "@agentsfleet/design-system";

import { auth } from "@clerk/nextjs/server";
import { listApprovals } from "@/lib/api/approvals";
import ApprovalsList from "./components/ApprovalsList";

export const dynamic = "force-dynamic";

const APPROVALS_DESCRIPTION = "Fleet actions that pause for human review.";

export default async function ApprovalsPage({
  params,
  searchParams,
}: {
  params: Promise<{ workspaceId: string }>;
  searchParams?: Promise<{ fleetId?: string }>;
}) {
  const { workspaceId } = await params;
  const { fleetId } = searchParams ? await searchParams : { fleetId: undefined };
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) redirect("/sign-in");

  // Header streams first; the inbox loads inside ApprovalsData under Suspense.
  return (
    <PageLayout>
      <PageHeader description={APPROVALS_DESCRIPTION}>
        <PageTitle>Approvals</PageTitle>
      </PageHeader>

      <Suspense fallback={<Skeleton className="h-48 rounded-lg" />}>
        <ApprovalsData workspaceId={workspaceId} fleetId={fleetId} />
      </Suspense>
    </PageLayout>
  );
}

// Async data region: loads the pending-approval inbox (workspace from the URL).
// Exported so it renders/tests in isolation.
export async function ApprovalsData({ workspaceId, fleetId }: { workspaceId: string; fleetId?: string }) {
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) return null;

  const initial = await listApprovals(workspaceId, token, { limit: 50, fleetId }).catch(
    () => ({ items: [], next_cursor: null }),
  );

  return (
    <Section asChild>
      <section aria-label="Pending approval gates">
        <SectionHeader>Review approvals</SectionHeader>
        <ApprovalsList
          workspaceId={workspaceId}
          initialItems={initial.items}
          initialCursor={initial.next_cursor}
          fleetId={fleetId}
        />
      </section>
    </Section>
  );
}
