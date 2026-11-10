// Test-only wire vocabulary for execution and preflight fixtures.
export const RUNNER_ADMIN_STATE = {active: "active", cordoned: "cordoned", draining: "draining", drained: "drained", revoked: "revoked"} as const;
export type RunnerAdminState = typeof RUNNER_ADMIN_STATE[keyof typeof RUNNER_ADMIN_STATE];
export const RUNNER_ADMIN_ACTION = {cordon: "cordon", drain: "drain", revoke: "revoke", self_test: "self_test"} as const;
export type RunnerAdminAction = typeof RUNNER_ADMIN_ACTION[keyof typeof RUNNER_ADMIN_ACTION];
export type RunnerEventType = "runner_registered" | "runner_online" | "runner_offline" | "lease_acquired" | "lease_released" | "runner_cordoned" | "runner_draining" | "runner_drained" | "runner_revoked" | "runner_token_rotated" | "runner_policy_assigned";
export type SandboxTier = "landlock_full" | "container_nested" | "dev_none";
export const LEASE_OUTCOME = {running: "running", succeeded: "succeeded", failed: "failed", expired: "expired", unknown: "unknown"} as const;
export const LEASE_KIND = {fresh: "fresh", reclaim: "reclaim"} as const;
export const LEASE_DESTINATION = {available: "available", fleetDeleted: "fleet_deleted", notPermitted: "not_permitted"} as const;
export interface AssignedPolicy {
  sandbox_tier: SandboxTier;
  network_policy: "allow_all" | "deny_all_egress" | "allow_list_egress";
  registry_allowlist: string[];
  worker_count: number;
  extra_binds: {path: string; mode: "read_only" | "read_write"; note: string}[];
  durable_tasks_enabled: boolean;
  decision_model_id?: string | null;
}
