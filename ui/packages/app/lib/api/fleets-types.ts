// Every fleet status the API can return. Dependency-free on purpose: client components read these
// without pulling the transport, whose retry policy is server-only.

// Every fleet status the API can return. Source of truth — every consumer
// that switches/compares against a status value reads from this const. Mirrors
// the backend `FleetStatus` enum in rustd/crates/daemon/afd_fleet_lifecycle/src/lib.rs.
export const AGENTSFLEET_STATUS = {
  ACTIVE: "active",
  // Terminal. An operator's archive (or the platform gate) cancels live work
  // and keeps the history readable; nothing returns the fleet to `active`.
  ARCHIVED: "archived",
  // The status a row is born in. Mirrors `FleetStatus::Installing` in
  // rustd/crates/daemon/afd_fleet_lifecycle/src/lib.rs — but no caller sees it on a
  // successful create any more: the flip to `active` happens inside the
  // install rather than on a thread after the 201
  // (rustd/crates/daemon/afd_fleet_lifecycle/src/install.rs), so the 201 already
  // reads `active`. The Fleets list/detail keep an installing indicator
  // visible while a fleet reads this, so a stalled install is never hidden.
  INSTALLING: "installing",
} as const;
export type FleetStatus = typeof AGENTSFLEET_STATUS[keyof typeof AGENTSFLEET_STATUS];

// The body of `POST /v1/workspaces/{ws}/fleets/{id}/messages`. Mirrors
// `af_wire::turn::SteerRequest`. `operation_id` is required here although
// the daemon accepts its absence: the dashboard always has an operation to
// name, and a send without one would run twice on a retried socket drop.
export type SteerRequest = {
  message: string;
  operation_id: string;
};

// The 202 a steer is answered with. Mirrors `af_wire::turn::SteerAccepted`:
// `replayed` is true when an earlier send of the same operation id already
// admitted the message, so `turn_id` is that send's turn and may have run.
export type SteerAccepted = {
  status: string;
  turn_id: string;
  replayed: boolean;
  // Where the message went. `folded`: the fleet's live run took it, and
  // `folded_into_turn_id` names that run; the message's own turn gets the
  // run's reply when that run reports. A daemon that predates the field sends
  // neither, and `postSteer` reads that as `started` and no run.
  disposition: SteerDisposition;
  folded_into_turn_id: string | null;
};

/** Mirrors `af_wire::turn::SteerDisposition`. */
export const STEER_DISPOSITION = {
  STARTED: "started",
  FOLDED: "folded",
  QUEUED: "queued",
} as const;
export type SteerDisposition = (typeof STEER_DISPOSITION)[keyof typeof STEER_DISPOSITION];

/** The same-origin route a browser steer is POSTed to: it mints the bearer the
 * browser does not hold (`app/live/v1/workspaces/[workspaceId]/fleets/[fleetId]/messages`). */
export function steerMessagesUrl(workspaceId: string, fleetId: string): string {
  return `/live/v1/workspaces/${encodeURIComponent(workspaceId)}/fleets/${encodeURIComponent(fleetId)}/messages`;
}
