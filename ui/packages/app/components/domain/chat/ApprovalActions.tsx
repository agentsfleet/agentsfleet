"use client";

import type { ResolveOutcome } from "@/lib/api/approvals";
import type { SummaryApproval } from "./use-fleet-approval";
import { useState } from "react";
import { Alert, Button, Time } from "@agentsfleet/design-system";
import { APPROVAL_DECISION } from "@/lib/api/approvals-types";
import { useApprovalDecision } from "../approvals/use-approval-decision";
import { ApprovalRequestDetails } from "./ApprovalRequestDetails";
import { PersonLabel } from "../PersonLabel";

type Props = {gate: SummaryApproval; onStart: () => void; onNext: () => void; onResolved: (fleetId: string) => void};

/** Approval is the decision itself; the action remains in this fleet's conversation. */
export function ApprovalActions({gate, onStart, onNext, onResolved}: Props) {
  const [outcome, setOutcome] = useState<ResolveOutcome | null>(null);
  const [reason, setReason] = useState("");
  const {pending, error, resolve} = useApprovalDecision(gate.workspace_id, gate.gate_id, result => {setOutcome(result); onResolved(gate.fleet_id);});
  const title = gate.proposed_action || `${gate.tool_name}:${gate.action_name}`;
  return <div className="space-y-md" data-approval-gate={gate.gate_id}>
    <p className="break-words text-label font-medium leading-label">{title}</p>
    {gate.blast_radius ? <p className="break-words text-label leading-label text-muted-foreground">{gate.blast_radius}</p> : null}
    <ApprovalRequestDetails gate={gate} pending={pending} decided={outcome !== null} reason={reason} onReasonChange={setReason} />
    {outcome ? <><output className="block text-label">Request {outcome.data.outcome.replaceAll("_", " ")}.</output>
      <p className="text-eyebrow text-muted-foreground">Decided <Time value={outcome.data.resolved_at} /> by <PersonLabel actor={outcome.data.resolved_by} name="" /></p>
      <Button variant="ghost" size="sm" onClick={onNext}>Check remaining requests</Button></> :
      <div className="grid grid-cols-2 gap-md">
        <Button className="min-h-11 summary:min-h-10" disabled={pending} aria-busy={pending}
          onClick={() => {onStart(); void resolve(APPROVAL_DECISION.APPROVE, reason);}}>Approve</Button>
        <Button variant="outline" className="min-h-11 border-border summary:min-h-10" disabled={pending} aria-busy={pending}
          onClick={() => {onStart(); void resolve(APPROVAL_DECISION.DENY, reason);}}>Deny</Button>
      </div>}
    {error ? <><Alert variant="destructive">{error}</Alert><Button variant="ghost" size="sm" onClick={onNext} disabled={pending}>Check status</Button></> : null}
  </div>;
}
