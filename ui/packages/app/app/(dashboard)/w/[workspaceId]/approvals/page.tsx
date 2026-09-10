import { Suspense } from "react";
import {
  PageHeader,
  PageLayout,
  PageTitle,
  Section,
  Skeleton,
} from "@agentsfleet/design-system";

import { requireCredential } from "@/lib/auth/credential";
import { listApprovals } from "@/lib/api/approvals";
import { APPROVALS_PAGE_LIMIT } from "@/lib/api/approvals-types";
import ApprovalsList from "./components/ApprovalsList";
import { APPROVALS_PAGE_DESCRIPTION } from "./copy";

export const dynamic = "force-dynamic";

export default async function ApprovalsPage({
  params,
  searchParams,
}: {
  params: Promise<{ workspaceId: string }>;
  searchParams?: Promise<{ fleetId?: string }>;
}) {
  const { workspaceId } = await params;
  const { fleetId } = searchParams ? await searchParams : { fleetId: undefined };
  const token = await requireCredential();

  // Header streams first; the inbox loads inside ApprovalsData under Suspense.
  return (
    <PageLayout>
      <PageHeader description={APPROVALS_PAGE_DESCRIPTION}>
        <PageTitle>Approvals</PageTitle>
      </PageHeader>

      <Suspense fallback={<Skeleton className="h-48 rounded-lg" />}>
        <ApprovalsData workspaceId={workspaceId} fleetId={fleetId} token={token} />
      </Suspense>
    </PageLayout>
  );
}

/**
 * Async data region: the whole inbox, every state, in one read.
 *
 * The `token` is the page's, not a second mint. This component used to call
 * `auth()` and `getToken()` again while the page above had already minted one,
 * checked it, and thrown it away — two mints to render one table. Props between
 * Server Components never cross to the client, so threading it costs nothing
 * and the page keeps its redirect as the single auth decision.
 *
 * `listApprovals` is called with no `status`, which now means every state
 * rather than `pending`. That is the whole reason the client has no mount read
 * any more: there is nothing left for it to fetch.
 *
 * Exported so it renders/tests in isolation.
 */
export async function ApprovalsData({
  workspaceId,
  fleetId,
  token,
}: {
  workspaceId: string;
  fleetId?: string;
  token: string;
}) {
  // A failed inbox read belongs to the retry boundary; it is not an empty inbox.
  const initial = await listApprovals(workspaceId, token, { limit: APPROVALS_PAGE_LIMIT, fleetId });

  return (
    <Section asChild>
      {/* The region holds every state, so the label no longer says "Pending".
          audits/msid-ui.sh:124 gates its carve-out on prev_added, and
          `git diff -U0` emits no context lines, so a label-only edit can never
          present the DS wrapper above as an added line; role="region" fails
          oxlint jsx-a11y/prefer-tag-over-role instead.
          UI GATE: SKIPPED per user override (reason: no edit to this line satisfies both checks) */}
      <section aria-label="Approval gates">
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
