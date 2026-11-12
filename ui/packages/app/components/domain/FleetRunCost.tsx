"use client";

import type { EventRow } from "@/lib/api/events";
import type { RunFigures } from "@/lib/events/run-summary";
import { useCallback, useSyncExternalStore } from "react";
import { readNanos } from "@/lib/api/nanos";
import { formatBalance } from "@/lib/money";
import { useOptionalFleetStreams } from "./FleetStreamProvider";
import { CONNECTION_STATUS } from "./useFleetEventStream";

const NO_EVENTS: EventRow[] = [];

/** Reuses the thread's run figures; an unreported cost stays absent. */
export function FleetRunCost({workspaceId, fleetId, initial = null}: {workspaceId: string; fleetId: string; initial?: RunFigures | null}) {
  const streams = useOptionalFleetStreams();
  const subscribe = useCallback((listener: () => void) => streams?.subscribe(workspaceId, fleetId, NO_EVENTS, listener) ?? (() => {}), [streams, workspaceId, fleetId]);
  const snapshot = useCallback(() => {
    const value = streams?.getSnapshot(fleetId);
    return value?.connectionStatus === CONNECTION_STATUS.REVOKED ? null : value?.latest ?? initial;
  }, [streams, fleetId, initial]);
  const serverSnapshot = useCallback(() => initial, [initial]);
  const run = useSyncExternalStore(subscribe, snapshot, serverSnapshot);
  const cost = readNanos(run?.cost_nanos);
  if (cost === null) return null;
  return <span className="hidden text-eyebrow leading-eyebrow text-muted-foreground summary-medium:inline" title="Reported cost of the latest run">{formatBalance(cost)} latest run</span>;
}
