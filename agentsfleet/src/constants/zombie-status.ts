// Zombie lifecycle status — wire-level enum mirroring
// `src/zombie/config_types.zig::ZombieStatus`. RULE UFS: every
// emit/compare site reads from here. `paused` is server-set
// (rate-limit / circuit-breaker); the CLI never mutates to it.

export const AGENTSFLEET_STATUS = Object.freeze({
  ACTIVE: "active",
  PAUSED: "paused",
  STOPPED: "stopped",
  KILLED: "killed",
});

export type ZombieStatus =
  (typeof AGENTSFLEET_STATUS)[keyof typeof AGENTSFLEET_STATUS];

// Status values the CLI is allowed to PATCH. `paused` is excluded
// because no CLI verb sets it.
export type ZombieMutationStatus = Exclude<
  ZombieStatus,
  typeof AGENTSFLEET_STATUS.PAUSED
>;
