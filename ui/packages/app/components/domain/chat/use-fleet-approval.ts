"use client";

import { useEffect, useState } from "react";
import { z } from "zod";
import { APPROVAL_STATUS } from "@/lib/api/approvals-types";
import { listApprovalsAction } from "@/lib/actions/fleet-approvals";

export type SummaryApproval = z.infer<typeof Gate>;
type ApprovalRead = {kind: "loading"} | {kind: "ready"; gate: SummaryApproval | null} | {kind: "failed" | "inaccessible"};
const Gate = z.object({gate_id: z.string().min(1), workspace_id: z.string(), fleet_id: z.string(),
  status: z.literal(APPROVAL_STATUS.PENDING), proposed_action: z.string(), tool_name: z.string(), action_name: z.string(), blast_radius: z.string(),
  detail: z.string().optional(), evidence: z.record(z.string(), z.unknown()).optional(), expires_at: z.iso.datetime({offset: true}).optional()});

/** One bounded read per observed gate change. Key the caller by authenticated scope. */
export function useFleetApproval(workspaceId: string, fleetId: string, revision: number, count: number) {
  const [read, setRead] = useState<ApprovalRead>({kind: "loading"});
  const [retryRevision, setRetryRevision] = useState(0);
  useEffect(() => {
    let current = true;
    setRead({kind: "loading"});
    void listApprovalsAction(workspaceId, {fleetId, status: "pending", limit: 1}).then(result => {
      if (!current) return;
      if (!result.ok) {setRead({kind: result.status === 401 || result.status === 403 ? "inaccessible" : "failed"}); return;}
      const parsed = z.array(Gate).max(1).safeParse(result.data.items);
      const gate = parsed.success ? parsed.data[0] ?? null : undefined;
      if (gate === undefined || gate && (gate.workspace_id !== workspaceId || gate.fleet_id !== fleetId)) {
        setRead({kind: "failed"}); return;
      }
      setRead({kind: "ready", gate});
    }).catch(() => {if (current) setRead({kind: "failed"});});
    return () => {current = false;};
  }, [workspaceId, fleetId, revision, count, retryRevision]);
  return {read, retry: () => setRetryRevision(value => value + 1)};
}
