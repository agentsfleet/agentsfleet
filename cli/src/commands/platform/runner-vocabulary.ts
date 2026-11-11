export enum RunnerState {
  Active = "active",
  Cordoned = "cordoned",
  Draining = "draining",
  Drained = "drained",
  Revoked = "revoked",
}

export enum RunnerAction {
  Cordon = "cordon",
  Drain = "drain",
  Revoke = "revoke",
  SelfTest = "self_test",
  Rotate = "rotate",
}

export enum RunnerLiveness {
  Registered = "registered",
  Busy = "busy",
  Online = "online",
  Offline = "offline",
}

export enum SandboxTier {
  LandlockFull = "landlock_full",
  ContainerNested = "container_nested",
  DevNone = "dev_none",
}

export enum RunnerNetworkPolicy {
  AllowAll = "allow_all",
  DenyAllEgress = "deny_all_egress",
  AllowListEgress = "allow_list_egress",
}

export enum BindMode {
  ReadOnly = "read_only",
  ReadWrite = "read_write",
}

export enum LeaseOutcome {
  Running = "running",
  Succeeded = "succeeded",
  Failed = "failed",
  Expired = "expired",
  Unknown = "unknown",
}

export enum LeaseKind {
  Fresh = "fresh",
  Reclaim = "reclaim",
}

export enum LeaseDestination {
  Available = "available",
  FleetDeleted = "fleet_deleted",
  NotPermitted = "not_permitted",
}

export enum RunnerHistory {
  Events = "events",
  Leases = "leases",
}

export enum RunnerEventType {
  Registered = "runner_registered",
  Online = "runner_online",
  Offline = "runner_offline",
  LeaseAcquired = "lease_acquired",
  LeaseReleased = "lease_released",
  Cordoned = "runner_cordoned",
  Draining = "runner_draining",
  Drained = "runner_drained",
  Revoked = "runner_revoked",
  TokenRotated = "runner_token_rotated",
  PolicyAssigned = "runner_policy_assigned",
}
