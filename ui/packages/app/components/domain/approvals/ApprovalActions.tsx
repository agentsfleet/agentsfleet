"use client";

import type { ApprovalDecision } from "@/lib/api/approvals-types";
import type { ResolveOutcome } from "@/lib/api/approvals";
import type { SummaryApproval } from "./use-fleet-approval";
import { useId, useState } from "react";
import { Alert, Button, Label, Textarea, Time, cn } from "@agentsfleet/design-system";
import { APPROVAL_DECISION, APPROVAL_GATE_TYPE, APPROVAL_STATUS } from "@/lib/api/approvals-types";
import { useApprovalDecision } from "./use-approval-decision";
import { ApprovalRequestDetails } from "./ApprovalRequestDetails";
import { FleetPayloadDisclosure } from "../activity/messages/FleetPayloadDisclosure";
import { PersonLabel } from "../shared/identity/PersonLabel";

type Props = {gate: SummaryApproval; inSetup: boolean; onStart: () => void; onNext: () => void; onResolved: (fleetId: string) => void};

const BUTTON = "min-h-11 summary:min-h-10";
const SECONDARY_BUTTON = `${BUTTON} border-border`;

/**
 * Approval is the decision itself; the action remains in this fleet's conversation.
 * An expired deadline leaves the recorded request actionable.
 */
export function ApprovalActions({gate, inSetup, onStart, onNext, onResolved}: Props) {
  const answerId = useId();
  const [outcome, setOutcome] = useState<ResolveOutcome | null>(null);
  const [reason, setReason] = useState("");
  const [answer, setAnswer] = useState("");
  const {pending, error, resolve} = useApprovalDecision(gate.workspace_id, gate.gate_id, result => {setOutcome(result); onResolved(gate.fleet_id);});
  const title = gate.proposed_action || `${gate.tool_name}:${gate.action_name}`;
  const question = gate.gate_type === APPROVAL_GATE_TYPE.QUESTION;
  const decide = (decision: ApprovalDecision, text: string) => {onStart(); void resolve(decision, text);};
  const expandedControls = inSetup || question;
  const deny = expandedControls && <Button variant="outline" className={SECONDARY_BUTTON} disabled={pending} aria-busy={pending}
    onClick={() => decide(APPROVAL_DECISION.DENY, reason)}>{question ? "Dismiss" : "Deny"}</Button>;
  return <div className="space-y-md" data-approval-gate={gate.gate_id}>
    <p className="break-words text-label font-medium leading-label">{title}</p>
    {gate.blast_radius ? <p className="break-words text-label leading-label text-muted-foreground">{gate.blast_radius}</p> : null}
    {gate.status === APPROVAL_STATUS.TIMED_OUT && !outcome ? <p className="text-label leading-label text-muted-foreground">Deadline passed</p> : null}
    {expandedControls ? (
      <ApprovalRequestDetails gate={gate} title={title} pending={pending} decided={outcome !== null} reason={reason} onReasonChange={setReason} />
    ) : <>
      {gate.detail ? <p className="break-words text-label leading-label">{gate.detail}</p> : null}
      {gate.expires_at && gate.status === APPROVAL_STATUS.PENDING && !outcome ? <p className="text-eyebrow text-muted-foreground">Deadline <Time value={gate.expires_at} /></p> : null}
      {gate.evidence && Object.keys(gate.evidence).length > 0 ? <FleetPayloadDisclosure json={JSON.stringify(gate.evidence)} /> : null}
    </>}
    {outcome ? <><output className="block text-label">Request {outcome.data.outcome.replaceAll("_", " ")}.</output>
      <p className="text-eyebrow text-muted-foreground">Decided <Time value={outcome.data.resolved_at} /> by <PersonLabel actor={outcome.data.resolved_by} name="" /></p>
      <Button variant="ghost" size="sm" onClick={onNext}>Check remaining requests</Button></> : question ?
      <div className="space-y-md">
        <Label htmlFor={answerId}>Your answer</Label>
        <Textarea id={answerId} value={answer} onChange={event => setAnswer(event.currentTarget.value)} rows={2} disabled={pending} />
        <div className="grid grid-cols-2 gap-md">
          <Button className={BUTTON} disabled={pending || !answer.trim()} aria-busy={pending}
            onClick={() => decide(APPROVAL_DECISION.ANSWER, answer)}>Send answer</Button>
          {deny}
        </div>
      </div> :
      <div className={cn("grid gap-md", inSetup && "grid-cols-2")}>
        <Button className={BUTTON} disabled={pending} aria-busy={pending}
          onClick={() => decide(APPROVAL_DECISION.APPROVE, reason)}>Approve</Button>
        {deny}
      </div>}
    {error ? <><Alert variant="destructive">{error}</Alert><Button variant="ghost" size="sm" onClick={onNext} disabled={pending}>Check status</Button></> : null}
  </div>;
}
