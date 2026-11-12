"use client";

import type { DraftView } from "@/lib/api/draft-state";
import { FleetApprovalSummary } from "./FleetApprovalSummary";

export function SetupApprovalSummary({view, workspace, subject, onResolved}: {view: DraftView; workspace: string; subject?: string; onResolved?: (fleetId: string) => void}) {
  if (!subject || !view.fleet || view.inaccessible) return null;
  return <FleetApprovalSummary key={`${subject}:${workspace}:${view.fleet.id}`} workspaceId={workspace}
    fleetId={view.fleet.id} revision={view.focusRevision + (view.activities?.length ?? 0)} count={0} onResolved={onResolved} />;
}
