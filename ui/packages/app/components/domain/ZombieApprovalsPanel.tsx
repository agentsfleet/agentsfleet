import { listApprovals } from "@/lib/api/approvals";
import ApprovalsList from "@/app/(dashboard)/approvals/components/ApprovalsList";

// Server-side wrapper that pre-fetches pending approvals scoped to one zombie
// and hands them to the same client list component used by /approvals. The
// client component's polling loop carries `zombieId` so revalidation stays
// scoped — the dashboard never refetches the full workspace queue from this
// panel.
export default async function ZombieApprovalsPanel({
  workspaceId,
  zombieId,
  token,
}: {
  workspaceId: string;
  zombieId: string;
  token: string;
}) {
  const initial = await listApprovals(workspaceId, token, {
    zombieId,
    limit: 50,
  }).catch(() => ({ items: [], next_cursor: null }));

  return (
    <ApprovalsList
      workspaceId={workspaceId}
      zombieId={zombieId}
      initialItems={initial.items}
      initialCursor={initial.next_cursor}
    />
  );
}
