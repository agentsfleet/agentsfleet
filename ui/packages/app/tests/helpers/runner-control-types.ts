// Wire values used by retained runner execution fixtures.
export enum RunnerState {
  Active = "active",
  Cordoned = "cordoned",
  Draining = "draining",
  Drained = "drained",
  Revoked = "revoked",
}

export enum RunnerLiveness {
  Registered = "registered",
  Busy = "busy",
  Online = "online",
  Offline = "offline",
}

export enum RunOutcome {
  Running = "running",
  Succeeded = "succeeded",
  Failed = "failed",
  Expired = "expired",
  Unknown = "unknown",
}

export enum RunKind {
  Fresh = "fresh",
  Reclaim = "reclaim",
}

export enum RunDestination {
  Available = "available",
  FleetDeleted = "fleet_deleted",
  NotPermitted = "not_permitted",
}
