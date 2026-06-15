import { listApprovals } from "@/lib/api/approvals";
import ApprovalsList from "@/app/(dashboard)/approvals/components/ApprovalsList";

// Server-side wrapper that pre-fetches pending approvals scoped to one agent
// and hands them to the same client list component used by /approvals. The
// client component's polling loop carries `agentId` so revalidation stays
// scoped — the dashboard never refetches the full workspace queue from this
// panel.
export default async function AgentApprovalsPanel({
  workspaceId,
  agentId,
  token,
}: {
  workspaceId: string;
  agentId: string;
  token: string;
}) {
  const initial = await listApprovals(workspaceId, token, {
    agentId,
    limit: 50,
  }).catch(() => ({ items: [], next_cursor: null }));

  return (
    <ApprovalsList
      workspaceId={workspaceId}
      agentId={agentId}
      initialItems={initial.items}
      initialCursor={initial.next_cursor}
    />
  );
}
