"use client";

import type { ResolveOutcome } from "@/lib/api/approvals";
import type { SummaryApproval } from "./use-fleet-approval";
import { useId, useState } from "react";
import { Alert, Button, Label, Textarea, Time } from "@agentsfleet/design-system";
import { APPROVAL_DECISION } from "@/lib/api/approvals-types";
import { useApprovalDecision } from "../approvals/use-approval-decision";
import { FleetPayloadDisclosure } from "../FleetPayloadDisclosure";
import { PersonLabel } from "../PersonLabel";

type Props = {gate: SummaryApproval; onStart: () => void; onNext: () => void; onResolved: (fleetId: string) => void};

/** Approval is the decision itself; the action remains in this fleet's conversation. */
export function ApprovalActions({gate, onStart, onNext, onResolved}: Props) {
  const [outcome, setOutcome] = useState<ResolveOutcome | null>(null);
  const [reason, setReason] = useState("");
  const reasonId = useId();
  const {pending, error, resolve} = useApprovalDecision(gate.workspace_id, gate.gate_id, result => {setOutcome(result); onResolved(gate.fleet_id);});
  const title = gate.proposed_action || `${gate.tool_name}:${gate.action_name}`;
  return <div className="space-y-md" data-approval-gate={gate.gate_id}>
    <p className="break-words text-label font-medium leading-label">{title}</p>
    {gate.blast_radius ? <p className="break-words text-label leading-label text-muted-foreground">{gate.blast_radius}</p> : null}
    <details className="group">
      <summary className="min-h-11 cursor-pointer rounded-sm py-md text-label text-muted-foreground hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring summary:min-h-9">Action details</summary>
      <div className="space-y-md pb-md text-label">
        <p className="break-words">{gate.tool_name} · {gate.action_name}</p>
        {gate.detail ? <p className="break-words">{gate.detail}</p> : null}
        {gate.evidence ? <FleetPayloadDisclosure json={JSON.stringify(gate.evidence, null, 2)} inline /> : null}
        {gate.expires_at ? <p>Expires <Time value={gate.expires_at} /></p> : null}
        {!outcome ? <div className="space-y-sm"><Label htmlFor={reasonId}>Reason (optional)</Label>
          <Textarea id={reasonId} value={reason} onChange={event => setReason(event.currentTarget.value)} maxLength={4096} rows={2} disabled={pending} /></div> : null}
      </div>
    </details>
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
