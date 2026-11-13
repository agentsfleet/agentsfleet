"use client";

import type { SummaryApproval } from "./use-fleet-approval";
import { useEffect, useState } from "react";
import { Button } from "@agentsfleet/design-system";
import { ApprovalActions } from "./ApprovalActions";
import { useFleetApproval } from "./use-fleet-approval";
import { WorkSection } from "./WorkSection";

type Props = {workspaceId: string; fleetId: string; revision: number; count: number; onResolved?: (fleetId: string) => void};

export function FleetApprovalSummary({workspaceId, fleetId, revision, count, onResolved}: Props) {
  const {read, retry} = useFleetApproval(workspaceId, fleetId, revision, count);
  const [selected, setSelected] = useState<{gate: SummaryApproval; decided: boolean} | null>(null);
  const inaccessible = read.kind === "inaccessible";
  useEffect(() => {if (inaccessible) setSelected(null);}, [inaccessible]);
  const gate = inaccessible ? null : selected?.gate ?? (read.kind === "ready" ? read.gate : null);
  const next = () => {setSelected(null); retry();};
  if (!gate && (read.kind === "ready" || read.kind === "loading" && count === 0)) return null;
  const needsApproval = gate ? !selected?.decided : count > 0;
  return <WorkSection id="fleet-approvals" title={needsApproval ? "Needs your approval" : gate ? "Approval decision" : "Approvals"}
    count={needsApproval && count > 0 ? count : undefined} attention={needsApproval}>
    {gate ? <ApprovalActions key={gate.gate_id} gate={gate} onStart={() => setSelected({gate, decided: false})} onNext={next}
      onResolved={fleet => {setSelected({gate, decided: true}); onResolved?.(fleet);}} /> : <div className="space-y-md">
      <p className="text-eyebrow text-muted-foreground">{read.kind === "loading" ? "Reading request…" : "Request details unavailable."}</p>
      {read.kind === "failed" || inaccessible ? <Button variant="ghost" size="sm" onClick={retry}>Try again</Button> : null}
    </div>}
  </WorkSection>;
}
