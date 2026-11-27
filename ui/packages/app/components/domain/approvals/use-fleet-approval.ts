"use client";

import { useEffect, useState } from "react";
import { z } from "zod";
import { Hostname } from "@/lib/api/fleet-detail";
import { APPROVAL_GATE_TYPE, APPROVAL_STATUS } from "@/lib/api/approvals-types";
import { listApprovalsAction } from "@/lib/actions/fleet-approvals";

const HostEvidence = z.object({
  kind: z.literal("host"), service: Hostname,
  redirect: z.object({tool: z.literal("web_fetch"), from_host: Hostname, to_host: Hostname}).nullish(),
}).refine(evidence => !evidence.redirect || evidence.redirect.to_host === evidence.service);

export type SummaryApproval = z.infer<typeof Gate> & {hostAccess?: z.infer<typeof HostEvidence>};
type ApprovalRead = {kind: "loading"} | {kind: "ready"; gate: SummaryApproval | null} | {kind: "failed" | "inaccessible"};
const Gate = z.object({gate_id: z.string().min(1), workspace_id: z.string(), fleet_id: z.string(),
  fleet_name: z.string().optional(), status: z.enum([APPROVAL_STATUS.PENDING, APPROVAL_STATUS.TIMED_OUT]), gate_type: z.string().optional(), proposed_action: z.string(), tool_name: z.string(), action_name: z.string(), blast_radius: z.string(),
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
      const parsed = z.array(Gate).safeParse(result.data.items);
      const gate = parsed.success ? safeGate(parsed.data[0]) : undefined;
      if (gate === undefined || gate && (gate.workspace_id !== workspaceId || gate.fleet_id !== fleetId)) {
        setRead({kind: "failed"}); return;
      }
      setRead({kind: "ready", gate});
    }).catch(() => {if (current) setRead({kind: "failed"});});
    return () => {current = false;};
  }, [workspaceId, fleetId, revision, count, retryRevision]);
  return {read, retry: () => setRetryRevision(value => value + 1)};
}

// A host approval displays only the grant target and the verified redirect's
// hostnames, including in the exact-action dialog. Payloads may contain URLs.
function safeGate(gate?: z.infer<typeof Gate>): SummaryApproval | null | undefined {
  if (!gate) return null;
  if (gate.gate_type !== APPROVAL_GATE_TYPE.ACCESS || gate.evidence?.kind !== "host") return gate;
  const parsed = HostEvidence.safeParse(gate.evidence);
  if (!parsed.success) return undefined;
  const host = parsed.data;
  return {...gate, hostAccess: host, evidence: host, tool_name: "request_access", action_name: host.service,
    proposed_action: `Allow ${gate.fleet_name || "this fleet"} to reach ${host.service}?`, blast_radius: "", detail: ""};
}
