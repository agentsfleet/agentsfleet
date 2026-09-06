import { type LiveFrame } from "@/lib/api/events";
import { FRAME_KIND } from "@/lib/api/events-types";
import { AGENTSFLEET_STATUS } from "@/lib/api/fleets-types";
import type { FleetFacts } from "@/lib/events/run-summary";
import { figure } from "./fleet-stream-row";

// What a live frame says about the FLEET rather than about a row: the
// lifecycle status a completion reports, and the pending-approval count the
// completion and both gate frames carry. Split from the frame reducer so that
// file stays about rows, and from the registry so the facts are testable
// without an EventSource.

// The statuses a fleet can be in. A completion's `fleet_status` is untrusted
// wire; a spelling outside this set — or an empty one — reads as unknown
// rather than as a status the strip would render and the page would refresh
// the server tree for.
const FLEET_STATUSES: ReadonlySet<string> = new Set(Object.values(AGENTSFLEET_STATUS));

/** A fleet status off the wire, or null for anything that is not one. */
function fleetStatus(value: unknown): string | null {
  return typeof value === "string" && FLEET_STATUSES.has(value) ? value : null;
}

/** What a frame that says nothing about the fleet carries: an empty patch. */
const NO_PATCH: Partial<FleetFacts> = Object.freeze({});

/**
 * The fleet facts one frame carries — an empty patch when it carries none. A
 * completion reports both; a gate frame reports the count alone, because a
 * gate opening or closing changes nothing about the fleet's lifecycle.
 */
export function factsOf(frame: LiveFrame): Partial<FleetFacts> {
  switch (frame.kind) {
    case FRAME_KIND.EVENT_COMPLETE:
      return {
        status: fleetStatus(frame.fleet_status),
        pendingApprovals: figure(frame.pending_approvals),
      };
    case FRAME_KIND.GATE_OPENED:
    case FRAME_KIND.GATE_RESOLVED:
      return { pendingApprovals: figure(frame.pending_approvals) };
    default:
      return NO_PATCH;
  }
}

/**
 * `current` with `patch` folded in — the same object when nothing changed, so
 * a subscriber keyed on identity does not re-render for a frame that restated
 * what it already showed. A null in the patch is "the frame did not say", and
 * never erases a fact already held.
 */
export function mergeFacts(current: FleetFacts, patch: Partial<FleetFacts>): FleetFacts {
  const status = patch.status ?? current.status;
  const pendingApprovals = patch.pendingApprovals ?? current.pendingApprovals;
  if (status === current.status && pendingApprovals === current.pendingApprovals) return current;
  return { status, pendingApprovals };
}
