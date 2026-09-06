// Every fleet status the API can return. Dependency-free on purpose: client components read these
// without pulling the transport, whose retry policy is server-only.

// Every fleet status the API can return. Source of truth — every consumer
// that switches/compares against a status value reads from this const. Mirrors
// the backend `FleetStatus` enum in rustd/crates/afd_fleet_lifecycle/src/lib.rs.
export const AGENTSFLEET_STATUS = {
  ACTIVE: "active",
  PAUSED: "paused",
  STOPPED: "stopped",
  KILLED: "killed",
  // The status a row is born in. Mirrors `FleetStatus::Installing` in
  // rustd/crates/afd_fleet_lifecycle/src/lib.rs — but no caller sees it on a
  // successful create any more: the flip to `active` happens inside the
  // install rather than on a thread after the 201
  // (rustd/crates/afd_fleet_lifecycle/src/install.rs), so the 201 already
  // reads `active`. The Fleets list/detail keep an installing indicator
  // visible while a fleet reads this, so a stalled install is never hidden.
  INSTALLING: "installing",
} as const;
export type FleetStatus = typeof AGENTSFLEET_STATUS[keyof typeof AGENTSFLEET_STATUS];
