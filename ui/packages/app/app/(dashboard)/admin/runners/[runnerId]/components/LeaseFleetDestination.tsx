"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import { Button, UnavailableState } from "@agentsfleet/design-system";
import { FleetDetailSchema } from "@/lib/api/fleet-detail";
import { losesAccess, setupRequest } from "@/lib/api/draft-request";
import { workspacePath } from "@/lib/workspace-routes";
import { OPEN_FLEET_LABEL } from "./runner-copy";

const DESTINATION = {loading: "loading", ready: "ready", unavailable: "unavailable", failed: "failed"} as const;
type Status = typeof DESTINATION[keyof typeof DESTINATION];
type Props = {workspaceId: string; fleetId: string};

export function LeaseFleetDestination({workspaceId, fleetId}: Props) {
  const target = `${workspaceId}:${fleetId}`;
  const [result, setResult] = useState<{target: string; status: Status}>({target, status: DESTINATION.loading});
  const status = result.target === target ? result.status : DESTINATION.loading;
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    const owner = new AbortController();
    void setupRequest({path: `/live/v1/workspaces/${encodeURIComponent(workspaceId)}/fleets/${encodeURIComponent(fleetId)}`},
      FleetDetailSchema, owner.signal).then((fleet) => {
      if (!owner.signal.aborted) setResult({target, status: fleet.id === fleetId ? DESTINATION.ready : DESTINATION.failed});
    }).catch((cause: unknown) => {
      if (!owner.signal.aborted) setResult({target, status: losesAccess(cause) ? DESTINATION.unavailable : DESTINATION.failed});
    });
    return () => owner.abort();
  }, [workspaceId, fleetId, target, attempt]);
  if (status === DESTINATION.loading) return <output className="text-sm text-muted-foreground">Checking fleet availability…</output>;
  if (status === DESTINATION.unavailable) return <p className="text-sm text-muted-foreground">This fleet is no longer available, or you don’t have access.</p>;
  if (status === DESTINATION.failed) return <UnavailableState title="Fleet availability couldn’t be checked"
    description="The lease is still available below. Retry to check its fleet."
    actions={<Button variant="secondary" size="sm" onClick={() => {setResult({target, status: DESTINATION.loading}); setAttempt((value) => value + 1);}}>Retry</Button>} />;
  return <Button asChild variant="outline" className="w-full sm:w-auto">
    <Link href={workspacePath(workspaceId, `fleets/${fleetId}`)}>{OPEN_FLEET_LABEL} →</Link>
  </Button>;
}
