"use client";

import type { EventRow } from "@/lib/api/events";
import type {Schedule} from "@/lib/api/draft-types";
import type {RunFigures} from "@/lib/events/run-summary";
import {cn} from "@agentsfleet/design-system";
import { useCallback, useSyncExternalStore } from "react";
import { useOptionalFleetStreams } from "./FleetStreamProvider";
import { CONNECTION_STATUS } from "./useFleetEventStream";

const NO_EVENTS: EventRow[] = [];

/** The same identity-owned registry as the thread; no separate API reader. */
export function FleetHeaderStatus({workspaceId, fleetId, initial, schedules = [], latestRun}: {workspaceId: string; fleetId: string; initial: number; schedules?: readonly Schedule[]; latestRun?: RunFigures | null}) {
  const streams = useOptionalFleetStreams();
  const subscribe = useCallback((listener: () => void) => streams?.subscribe(workspaceId, fleetId, NO_EVENTS, listener) ?? (() => {}), [streams, workspaceId, fleetId]);
  const snapshot = useCallback(() => {
    const value = streams?.getSnapshot(fleetId);
    return value?.connectionStatus === CONNECTION_STATUS.REVOKED ? null : labelFor(value?.fleet.pendingApprovals ?? initial, value?.latest ?? latestRun, schedules);
  }, [streams, fleetId, initial, latestRun, schedules]);
  const serverSnapshot = useCallback(() => labelFor(initial, latestRun, schedules), [initial, latestRun, schedules]);
  const label = useSyncExternalStore(subscribe, snapshot, serverSnapshot);
  if (!label) return null;
  return <span className={cn("hidden shrink-0 items-center gap-md text-eyebrow leading-eyebrow summary:inline-flex", label === "Scheduled" ? "text-muted-foreground" : "text-warning")}>
    <span className="size-1.5 rounded-full bg-current" aria-hidden="true" /><span className="sr-only summary:not-sr-only">{label}</span>
  </span>;
}

function labelFor(approvals: number, run: RunFigures | null | undefined, schedules: readonly Schedule[]) {
  if (approvals > 0) return "Needs approval";
  if (run?.status === "fleet_error" || schedules.some(schedule => schedule.sync === "failed")) return "Needs attention";
  return schedules.some(schedule => schedule.status === "active" && schedule.sync === "synced" && schedule.run_state === "scheduled" && schedule.next_run_at) ? "Scheduled" : null;
}
