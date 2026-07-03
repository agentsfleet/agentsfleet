"use client";

import { useEffect, useRef } from "react";
import { captureProductEvent } from "@/lib/analytics/posthog";
import { EVENTS } from "@/lib/analytics/events";

// Fires fleet_viewed once per fleet the operator opens. The detail page is a
// Server Component, so this thin client child owns the client-only capture. The
// effect keys on the fleet id (a fresh route mount per fleet), so navigating
// between fleets re-fires while re-renders of the same fleet do not; the status
// is read through a ref so a mid-view status flip does not double-fire.
export function FleetViewedTracker({ fleetId, status }: { fleetId: string; status: string }) {
  const statusRef = useRef(status);
  statusRef.current = status;
  useEffect(() => {
    captureProductEvent(EVENTS.fleet_viewed, { fleet_id: fleetId, status: statusRef.current });
  }, [fleetId]);
  return null;
}
