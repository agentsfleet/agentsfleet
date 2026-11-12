"use client";

import type { EventRow } from "@/lib/api/events";
import { useCallback, useSyncExternalStore } from "react";
import { useOptionalFleetStreams } from "./FleetStreamProvider";
import { CONNECTION_STATUS } from "./useFleetEventStream";

const NO_EVENTS: EventRow[] = [];

/** The same identity-owned registry as the thread; no separate API reader. */
export function FleetApprovalStatus({workspaceId, fleetId, initial}: {workspaceId: string; fleetId: string; initial: number}) {
  const streams = useOptionalFleetStreams();
  const subscribe = useCallback((listener: () => void) => streams?.subscribe(workspaceId, fleetId, NO_EVENTS, listener) ?? (() => {}), [streams, workspaceId, fleetId]);
  const snapshot = useCallback(() => {
    const value = streams?.getSnapshot(fleetId);
    return value?.connectionStatus === CONNECTION_STATUS.REVOKED ? 0 : value?.fleet.pendingApprovals ?? initial;
  }, [streams, fleetId, initial]);
  const serverSnapshot = useCallback(() => initial, [initial]);
  const count = useSyncExternalStore(subscribe, snapshot, serverSnapshot);
  if (!count) return null;
  return <span className="inline-flex shrink-0 items-center gap-md text-eyebrow leading-eyebrow text-warning"><span className="size-1.5 rounded-full bg-warning" aria-hidden="true" /><span className="sr-only summary:not-sr-only">Needs approval</span></span>;
}
