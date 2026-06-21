import { listApprovals } from "@/lib/api/approvals";
import ApprovalsList from "@/app/(dashboard)/approvals/components/ApprovalsList";

// Server-side wrapper that pre-fetches pending approvals scoped to one fleet
// and hands them to the same client list component used by /approvals. The
// client component's polling loop carries `fleetId` so revalidation stays
// scoped — the dashboard never refetches the full workspace queue from this
// panel.
export default async function FleetApprovalsPanel({
  workspaceId,
  fleetId,
  token,
}: {
  workspaceId: string;
  fleetId: string;
  token: string;
}) {
  const initial = await listApprovals(workspaceId, token, {
    fleetId,
    limit: 50,
  }).catch(() => ({ items: [], next_cursor: null }));

  return (
    <ApprovalsList
      workspaceId={workspaceId}
      fleetId={fleetId}
      initialItems={initial.items}
      initialCursor={initial.next_cursor}
    />
  );
}
