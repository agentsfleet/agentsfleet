import type { LiveFrame } from "@/lib/api/events";
import type { FRAME_KIND } from "@/lib/api/events-types";
import { advancesContext, readContextProgress } from "../context-progress";
import { AGENTSFLEET_EVENT_STATUS, type FleetEvent } from "./fleet-stream-row";

/** A status hint can affect only its already-running row, never durable history. */
export function applyContextProgress(prev: FleetEvent[], frame: Extract<LiveFrame, { kind: typeof FRAME_KIND.CONTEXT_PROGRESS }>): FleetEvent[] {
  const progress = readContextProgress({ generation: frame.generation, state: frame.state });
  const index = prev.findIndex(event => event.id === frame.event_id);
  const current = prev[index];
  if (!progress || !current || current.status !== AGENTSFLEET_EVENT_STATUS.RECEIVED
    || !advancesContext(current.contextProgress, progress)) return prev;
  const next = [...prev];
  next[index] = { ...current, contextProgress: progress, contextProgressHidden: undefined };
  return next;
}

/** Reconnects discard the visible hint while retaining its ordering frontier. */
export function hideContextProgress(prev: FleetEvent[]): FleetEvent[] {
  const next = prev.map(event => {
    if (!event.contextProgress || event.contextProgressHidden) return event;
    return { ...event, contextProgressHidden: true as const };
  });
  return next.some((event, index) => event !== prev[index]) ? next : prev;
}
