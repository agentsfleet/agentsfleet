"use client";

import { useState } from "react";
import nextDynamic from "next/dynamic";
import { Button } from "@agentsfleet/design-system";
import { TURN_STATUS } from "@/lib/turns/turn-summary";
import { useFleetScope } from "../stream/FleetScope";

const TurnDetailsDialog = nextDynamic(() => import("../../turns/TurnDetailsDialog").then(module => module.TurnDetailsDialog), {ssr: false});

/** A queued turn has no record to inspect until a run is assigned it. */
export function FleetRunDetails({turnId, status}: {turnId: string; status: string}) {
  const scope = useFleetScope();
  const [open, setOpen] = useState(false);
  if (!scope || !Object.values(TURN_STATUS).some(value => value === status)) return null;
  return <>
    <Button variant="link" size="sm" className="min-h-11 px-sm text-conversation text-muted-foreground sm:min-h-9" onClick={() => setOpen(true)}>Run details</Button>
    {open ? <TurnDetailsDialog row={{workspace_id: scope.workspaceId, fleet_id: scope.fleetId, turn_id: turnId}}
      refresh={status} onOpenChange={setOpen} /> : null}
  </>;
}
