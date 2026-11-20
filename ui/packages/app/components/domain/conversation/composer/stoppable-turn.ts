import { AGENTSFLEET_EVENT_STATUS, type FleetEvent } from "@/lib/streaming/fleet-stream-row";

// Which turn the composer's Stop ends: the one the fleet is working on. A run
// in progress comes first (`received`, or a send `folded` into it), because
// stopping it ends every turn the run serves; with none running, the oldest
// turn still waiting (queued, or parked on an approval) is next in line. A row
// this tab painted before the daemon named it has no turn to address yet.
const RUNNING: ReadonlySet<string> = new Set([
  AGENTSFLEET_EVENT_STATUS.RECEIVED,
  AGENTSFLEET_EVENT_STATUS.FOLDED,
]);

/** A refused-at-gate row carries its failure label; a parked one waits for a person. */
function waiting(event: FleetEvent): boolean {
  if (event.status === AGENTSFLEET_EVENT_STATUS.QUEUED) return event.clientTimestamp !== true;
  return event.status === AGENTSFLEET_EVENT_STATUS.GATE_BLOCKED && event.failureLabel === null;
}

/** The turn id Stop addresses, or null when no turn of this fleet runs. */
export function stoppableTurn(events: readonly FleetEvent[]): string | null {
  return events.find((event) => RUNNING.has(event.status))?.id
    ?? events.find(waiting)?.id
    ?? null;
}
