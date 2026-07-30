"use client";

import { useEffect, useRef } from "react";
import { captureProductEvent } from "@/lib/analytics/posthog";
import { EVENTS } from "@/lib/analytics/events";

// Fires runner_viewed once per runner the operator opens, mirroring
// FleetViewedTracker. The detail page is a Server Component, so this thin
// client child owns the client-only capture; the coarse states ride a ref so a
// mid-view flip does not double-fire. Never the host identifier, a token or
// label values — the catalog's allowlist enforces the property set.
export function RunnerViewedTracker({
  runnerId,
  liveness,
  adminState,
}: {
  runnerId: string;
  liveness: string;
  adminState: string;
}) {
  const livenessRef = useRef(liveness);
  livenessRef.current = liveness;
  const adminStateRef = useRef(adminState);
  adminStateRef.current = adminState;
  useEffect(() => {
    captureProductEvent(EVENTS.runner_viewed, {
      runner_id: runnerId,
      liveness: livenessRef.current,
      admin_state: adminStateRef.current,
    });
  }, [runnerId]);
  return null;
}
