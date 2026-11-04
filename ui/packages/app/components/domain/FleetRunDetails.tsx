"use client";

import { useState } from "react";
import nextDynamic from "next/dynamic";
import { Button } from "@agentsfleet/design-system";
import { EVENT_STATUS } from "@/lib/events/event-summary";
import { useFleetScope } from "./FleetScope";

const EventDetailsDialog = nextDynamic(() => import("./EventDetailsDialog").then(module => module.EventDetailsDialog), {ssr: false});

/** Queued admissions have no event record to inspect until a lease promotes them. */
export function FleetRunDetails({eventId, status}: {eventId: string; status: string}) {
  const scope = useFleetScope();
  const [open, setOpen] = useState(false);
  if (!scope || !Object.values(EVENT_STATUS).some(value => value === status)) return null;
  return <>
    <Button variant="link" size="sm" className="min-h-11 px-sm text-conversation sm:min-h-9" onClick={() => setOpen(true)}>Run details</Button>
    {open ? <EventDetailsDialog row={{workspace_id: scope.workspaceId, fleet_id: scope.fleetId, event_id: eventId}}
      refresh={status} onOpenChange={setOpen} /> : null}
  </>;
}
