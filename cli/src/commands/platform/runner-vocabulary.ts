export enum CordonReason {
  MissedHeartbeats = "missed_heartbeats",
}

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

/** The engine that builds a runner's sandboxes, as its heartbeat reports it. */
export enum SandboxEngine {
  Firecracker = "firecracker",
  Unsandboxed = "unsandboxed",
}

/** The wall a runner's engine puts between a sandbox's code and the host. */
export enum Isolation {
  Unconfined = "unconfined",
  HardwareVm = "hardware_vm",
}

/** What a runner host's virtualization device offers. */
export enum Kvm {
  Usable = "usable",
  Denied = "denied",
  Absent = "absent",
}

/** The isolation a runner is assigned, named for the engine that delivers it. */
export enum SandboxTier {
  Firecracker = "firecracker",
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

export enum RunOutcome {
  Running = "running",
  Succeeded = "succeeded",
  Failed = "failed",
  Expired = "expired",
  Stopped = "stopped",
  Unknown = "unknown",
}

export enum RunKind {
  Fresh = "fresh",
  Reassigned = "reassigned",
}

export enum RunDestination {
  Available = "available",
  FleetDeleted = "fleet_deleted",
  NotPermitted = "not_permitted",
}

export enum RunnerHistory {
  History = "history",
  Runs = "runs",
}

export enum RunnerEventType {
  Registered = "runner_registered",
  Online = "runner_online",
  Offline = "runner_offline",
  RunAcquired = "run_acquired",
  RunReleased = "run_released",
  Cordoned = "runner_cordoned",
  Uncordoned = "runner_uncordoned",
  Draining = "runner_draining",
  Drained = "runner_drained",
  Revoked = "runner_revoked",
  TokenRotated = "runner_token_rotated",
  PolicyAssigned = "runner_policy_assigned",
}
