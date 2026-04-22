/** Domain types — mirrors zombied API contracts */

export type RunStatus =
  | "SPEC_QUEUED"
  | "RUN_PLANNED"
  | "PATCH_IN_PROGRESS"
  | "VERIFICATION_IN_PROGRESS"
  | "PR_PREPARED"
  | "PR_OPENED"
  | "NOTIFIED"
  | "DONE"
  | "FAILED"
  | "RETRYING";

export type CommandClass = "safe" | "sensitive" | "critical";

export type Workspace = {
  id: string;
  name: string;
  repo_url: string;
  paused: boolean;
  created_at: string;
  run_count: number;
  last_run_at: string | null;
  plan: "hobby" | "pro" | "team" | "enterprise";
};

export type Run = {
  id: string;
  workspace_id: string;
  spec_path: string;
  status: RunStatus;
  attempts: number;
  max_attempts: number;
  created_at: string;
  updated_at: string;
  duration_seconds: number | null;
  pr_url: string | null;
  artifacts: RunArtifacts | null;
  error: string | null;
};

export type RunArtifacts = {
  plan: string | null;
  implementation: string | null;
  validation: string | null;
  summary: string | null;
  defect_report: string | null;
};

export type RunTransition = {
  id: string;
  run_id: string;
  from_status: RunStatus | null;
  to_status: RunStatus;
  reason: string;
  actor: string;
  created_at: string;
};

export type Spec = {
  id: string;
  workspace_id: string;
  path: string;
  status: "PENDING" | "QUEUED" | "RUNNING" | "DONE" | "FAILED";
  created_at: string;
};

export type ApiError = {
  error: string;
  code: string;
  status: number;
};

export type PaginatedResponse<T> = {
  data: T[];
  has_more: boolean;
  next_cursor: string | null;
  request_id: string;
};

// ── Zombies (M19_001) ──

export type Zombie = {
  id: string;
  name: string;
  status: string;
  created_at: number;
  updated_at: number;
};

export type InstallZombieRequest = {
  name: string;
  source_markdown: string;
  config_json: string;
};

export type InstallZombieResponse = {
  zombie_id: string;
  status: string;
};

export type ZombieListResponse = {
  items: Zombie[];
  total: number;
};

// ── Tenant billing (M11_005 + M11_006) ──

// Unix-epoch timestamps on this type are **milliseconds**, matching the
// server's `*_at_ms` fields (src/state/tenant_billing_store.zig). Pass them
// straight to `new Date(n)`; never multiply by 1000.
export type TenantBilling = {
  plan_tier: string;
  plan_sku: string;
  balance_cents: number;
  updated_at: number;
  is_exhausted: boolean;
  exhausted_at: number | null;
};
