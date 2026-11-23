import { AGENTSFLEET_TURN_STATUS, type FleetTurn } from "@/lib/streaming/fleet-stream-row";

// Which turn the composer's Stop ends: the one the fleet is working on. A run
// in progress comes first (`received`, or a send `folded` into it), because
// stopping it ends every turn the run serves; with none running, the oldest
// turn still waiting (queued, or parked on an approval) is next in line. A row
// this tab painted before the daemon named it has no turn to address yet.
const RUNNING: ReadonlySet<string> = new Set([
  AGENTSFLEET_TURN_STATUS.RECEIVED,
  AGENTSFLEET_TURN_STATUS.FOLDED,
]);

/** A refused-at-gate row carries its failure label; a parked one waits for a person. */
function waiting(turn: FleetTurn): boolean {
  if (turn.status === AGENTSFLEET_TURN_STATUS.QUEUED) return turn.clientTimestamp !== true;
  return turn.status === AGENTSFLEET_TURN_STATUS.GATE_BLOCKED && turn.failureLabel === null;
}

/** The turn id Stop addresses, or null when no turn of this fleet runs. */
export function stoppableTurn(turns: readonly FleetTurn[]): string | null {
  return turns.find((turn) => RUNNING.has(turn.status))?.id
    ?? turns.find(waiting)?.id
    ?? null;
}
