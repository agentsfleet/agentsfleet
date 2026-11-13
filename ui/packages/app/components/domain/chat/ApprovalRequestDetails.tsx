"use client";

import type { SummaryApproval } from "./use-fleet-approval";
import { useId, useState } from "react";
import { ArrowUpRightIcon } from "@phosphor-icons/react/ssr";
import { Button, Dialog, DialogContent, DialogDescription, DialogFooter,
  DialogHeader, DialogTitle, DialogTrigger, Label, Textarea, Time } from "@agentsfleet/design-system";
import { FleetPayloadDisclosure } from "../FleetPayloadDisclosure";

type Props = {gate: SummaryApproval; pending: boolean; decided: boolean; reason: string; onReasonChange: (value: string) => void};

/** Inspect the recorded request while the conversation retains decision ownership. */
export function ApprovalRequestDetails({gate, pending, decided, reason, onReasonChange}: Props) {
  const reasonId = useId();
  const [open, setOpen] = useState(false);
  return <Dialog open={open} onOpenChange={setOpen}>
    <DialogTrigger asChild><Button variant="ghost" size="compact" className="justify-start px-0" disabled={pending}>
      View exact action<ArrowUpRightIcon size={16} aria-hidden="true" />
    </Button></DialogTrigger>
    <DialogContent>
      <DialogHeader>
        <DialogTitle>Exact action</DialogTitle>
        <DialogDescription>“{gate.proposed_action || `${gate.tool_name}:${gate.action_name}`}”</DialogDescription>
      </DialogHeader>
      <div className="space-y-xl text-label">
        <p className="break-words">{gate.tool_name} · {gate.action_name}</p>
        {gate.blast_radius ? <p className="break-words text-muted-foreground">{gate.blast_radius}</p> : null}
        {gate.detail ? <p className="break-words">{gate.detail}</p> : null}
        {gate.evidence ? <FleetPayloadDisclosure json={JSON.stringify(gate.evidence, null, 2)} inline /> : null}
        {gate.expires_at ? <p>Expires <Time value={gate.expires_at} /></p> : null}
        {!decided ? <div className="space-y-md"><Label htmlFor={reasonId}>Reason (optional)</Label>
          <Textarea id={reasonId} value={reason} onChange={event => onReasonChange(event.currentTarget.value)} maxLength={4096} rows={2} disabled={pending} /></div> : null}
      </div>
      <DialogFooter><Button variant="outline" onClick={() => setOpen(false)}>Done</Button></DialogFooter>
    </DialogContent>
  </Dialog>;
}
