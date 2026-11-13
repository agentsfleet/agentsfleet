// Fleet lifecycle status — wire-level enum mirroring the `FleetStatus`
// spellings in `rustd/crates/daemon/afd_fleet_lifecycle/src/lib.rs`.
// RULE Unnamed String (UFS): every emit/compare site reads from here.
// `archived` is terminal and keeps history; deleting removes the fleet in any
// status, so it is a request, not a status.

export const AGENTSFLEET_STATUS = Object.freeze({
  INSTALLING: "installing",
  ACTIVE: "active",
  ARCHIVED: "archived",
});

export type FleetStatus =
  (typeof AGENTSFLEET_STATUS)[keyof typeof AGENTSFLEET_STATUS];
