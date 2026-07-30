import { request } from "./client";

// host_id is free-form but bounded by the backend; deriving HOST_ID_REGEX from
// HOST_ID_MAX keeps the form in step with `register.zig`'s MAX_HOST_ID_LEN as a
// single source — the bound lives in exactly one place.
export const HOST_ID_MAX = 256;
export const HOST_ID_REGEX = new RegExp(`^[A-Za-z0-9_.-]{1,${HOST_ID_MAX}}$`);
export const LABEL_REGEX = /^[A-Za-z0-9_.-]{1,64}$/;

// Self-reported isolation strength — mirrors `protocol.SandboxTier` verbatim
// (UFS: the tag names are the wire shape). `dev_none` is dev-only; a release
// daemon refuses it at boot.
export const SANDBOX_TIERS = ["landlock_full", "container_nested", "macos_seatbelt", "dev_none"] as const;
export type SandboxTier = (typeof SANDBOX_TIERS)[number];

// Operator-facing labels for the self-reported isolation stack — the raw enum
// tags are the wire shape (sent verbatim), these are the human strings the
// dropdown and list render. Keyed so a new tier can't be added without a label.
export const SANDBOX_TIER_LABELS: Record<SandboxTier, string> = {
  landlock_full: "Linux · Landlock (full)",
  container_nested: "Nested container",
  macos_seatbelt: "macOS · Seatbelt",
  dev_none: "None (dev only)",
};

// One-line descriptions for the isolation-mode OptionCard picker. Keyed by
// the same SandboxTier so a new tier can't be added without one. Mirrors
// docs/architecture/runner_fleet.md §Sandbox tiers — do not restate from the
// label alone; that table is the source of truth for what each tier means.
export const SANDBOX_TIER_DESCRIPTIONS: Record<SandboxTier, string> = {
  landlock_full: "Bare Linux host with kernel-level Landlock sandboxing — full isolation, eligible for any work.",
  container_nested: "Runner runs inside a container on a Linux host or VM — same full-isolation tier as Landlock.",
  macos_seatbelt: "macOS's Seatbelt sandbox — weaker isolation; limited to your own workspace's dev work.",
  dev_none: "No real sandbox — for local development builds only; a non-debug runner build refuses to start with this tier.",
};

// Derived runtime liveness — mirrors `protocol.RunnerLiveness` tag names. Never
// stored; computed server-side from last_seen_at + the live-lease join.
export const RUNNER_LIVENESS = ["registered", "busy", "online", "offline"] as const;
export type RunnerLiveness = (typeof RUNNER_LIVENESS)[number];

export const RUNNER_ADMIN_STATE = {
  active: "active",
  cordoned: "cordoned",
  draining: "draining",
  drained: "drained",
  revoked: "revoked",
} as const;
export type RunnerAdminState = (typeof RUNNER_ADMIN_STATE)[keyof typeof RUNNER_ADMIN_STATE];
export const RUNNER_ADMIN_STATES = [
  RUNNER_ADMIN_STATE.active,
  RUNNER_ADMIN_STATE.cordoned,
  RUNNER_ADMIN_STATE.draining,
  RUNNER_ADMIN_STATE.drained,
  RUNNER_ADMIN_STATE.revoked,
] as const;

export const RUNNER_ADMIN_ACTION = {
  cordon: "cordon",
  drain: "drain",
  revoke: "revoke",
} as const;
export type RunnerAdminAction = (typeof RUNNER_ADMIN_ACTION)[keyof typeof RUNNER_ADMIN_ACTION];
export const RUNNER_ADMIN_ACTIONS = [
  RUNNER_ADMIN_ACTION.cordon,
  RUNNER_ADMIN_ACTION.drain,
  RUNNER_ADMIN_ACTION.revoke,
] as const;

export const RUNNER_EVENT_TYPES = [
  "runner_registered",
  "runner_online",
  "runner_offline",
  "lease_acquired",
  "lease_released",
  "runner_cordoned",
  "runner_draining",
  "runner_drained",
  "runner_revoked",
] as const;
export type RunnerEventType = (typeof RUNNER_EVENT_TYPES)[number];

// The lifecycle subset Activity renders: every tag EXCEPT the two per-work
// records, which the Leases table already states once each with an outcome.
// One exported constant, consumed verbatim by the Activity caller (Invariant:
// lifecycle and work events never mix in that feed).
export const RUNNER_LIFECYCLE_EVENT_TYPES = [
  "runner_registered",
  "runner_online",
  "runner_offline",
  "runner_cordoned",
  "runner_draining",
  "runner_drained",
  "runner_revoked",
] as const satisfies readonly RunnerEventType[];

// Canonical Stripe-style paging parameter names — spelled identically to the
// daemon's `QUERY_STARTING_AFTER` / `QUERY_LIMIT` (http/pagination.zig).
export const QUERY_STARTING_AFTER = "starting_after";
export const QUERY_LIMIT = "limit";

const FLEET_RUNNERS_PATH = "/v1/fleets/runners";
const RUNNERS_ENROLLMENT_PATH = "/v1/runners";

/// The "never contacted" sentinel for `last_seen_at`, mirroring
/// `protocol.RUNNER_LAST_SEEN_NEVER` — same name across both runtimes so the
/// pair stays greppable. A runner is minted with this at registration and
/// carries it until its first heartbeat, so it is a real state, not a null.
export const RUNNER_LAST_SEEN_NEVER = 0;

export interface RunnerListItem {
  id: string;
  host_id: string;
  sandbox_tier: SandboxTier;
  admin_state: RunnerAdminState;
  liveness: RunnerLiveness;
  labels: string[];
  last_seen_at: number;
  created_at: number;
}

export interface RunnerListResponse {
  items: RunnerListItem[];
  total: number | null;
  next_cursor: string | null;
}

/** The single-runner operator read: the list fields plus live-work and lifetime counters. */
export interface RunnerDetail extends RunnerListItem {
  active_lease_count: number;
  active_fleet_count: number;
  leases_acquired: number;
  leases_succeeded: number;
  leases_failed: number;
  leases_expired: number;
}

// Settled server-side into one closed tag; the client never re-derives an
// outcome from raw statuses (the two surfaces cannot drift on what expired means).
export const LEASE_OUTCOME = {
  running: "running",
  succeeded: "succeeded",
  failed: "failed",
  expired: "expired",
  unknown: "unknown",
} as const;
export type LeaseOutcome = (typeof LEASE_OUTCOME)[keyof typeof LEASE_OUTCOME];

export const LEASE_KIND = {
  fresh: "fresh",
  reclaim: "reclaim",
} as const;
export type LeaseKind = (typeof LEASE_KIND)[keyof typeof LEASE_KIND];

export interface RunnerLease {
  id: string;
  fleet_id: string;
  fleet_name: string | null;
  workspace_id: string;
  event_id: string;
  event_type: string;
  actor: string;
  outcome: LeaseOutcome;
  failure_label: string | null;
  failure_detail: string | null;
  kind: LeaseKind;
  fencing_token: number;
  provider: string;
  model: string;
  posture: string;
  metered_input_tokens: number;
  metered_cached_tokens: number;
  metered_output_tokens: number;
  wall_ms: number | null;
  lease_expires_at: number;
  created_at: number;
}

export interface RunnerLeaseResponse {
  items: RunnerLease[];
  total: number | null;
  next_cursor: string | null;
}

/** The mint response — `runner_token` is the raw `agt_r`, returned exactly once. */
export interface CreatedRunner {
  runner_id: string;
  runner_token: string;
}

export interface RunnerAdminStateUpdate {
  id: string;
  admin_state: RunnerAdminState;
}

export interface RunnerEventItem {
  id: string;
  runner_id: string;
  event_type: RunnerEventType;
  occurred_at: number;
  metadata: unknown;
}

export interface RunnerEventsResponse {
  items: RunnerEventItem[];
  total: number | null;
  next_cursor: string | null;
}

export interface ListParams {
  starting_after?: string;
  limit?: number;
}

export interface EventListParams {
  starting_after?: string;
  limit?: number;
  /** One tag, or a comma-separated set returning the union. */
  event_type?: string;
  since?: number;
  until?: number;
}

export interface LeaseListParams {
  starting_after?: string;
  limit?: number;
}

function keysetParams(params: ListParams): URLSearchParams {
  const qs = new URLSearchParams();
  if (params.starting_after) qs.set(QUERY_STARTING_AFTER, params.starting_after);
  if (params.limit !== undefined) qs.set(QUERY_LIMIT, String(params.limit));
  return qs;
}

export async function listRunners(token: string, params: ListParams = {}): Promise<RunnerListResponse> {
  const qs = keysetParams(params);
  const suffix = qs.size > 0 ? `?${qs.toString()}` : "";
  return request<RunnerListResponse>(`${FLEET_RUNNERS_PATH}${suffix}`, { method: "GET" }, token);
}

export async function getRunner(token: string, runnerId: string): Promise<RunnerDetail> {
  return request<RunnerDetail>(`${FLEET_RUNNERS_PATH}/${encodeURIComponent(runnerId)}`, { method: "GET" }, token);
}

export async function listRunnerLeases(
  token: string,
  runnerId: string,
  params: LeaseListParams = {},
): Promise<RunnerLeaseResponse> {
  const qs = keysetParams(params);
  const suffix = qs.size > 0 ? `?${qs.toString()}` : "";
  return request<RunnerLeaseResponse>(
    `${FLEET_RUNNERS_PATH}/${encodeURIComponent(runnerId)}/leases${suffix}`,
    { method: "GET" },
    token,
  );
}

export async function createRunner(
  token: string,
  body: { host_id: string; sandbox_tier: SandboxTier; labels: string[] },
): Promise<CreatedRunner> {
  return request<CreatedRunner>(RUNNERS_ENROLLMENT_PATH, { method: "POST", body: JSON.stringify(body) }, token);
}

export async function updateRunnerAdminState(
  token: string,
  runnerId: string,
  action: RunnerAdminAction,
): Promise<RunnerAdminStateUpdate> {
  return request<RunnerAdminStateUpdate>(
    `${FLEET_RUNNERS_PATH}/${encodeURIComponent(runnerId)}`,
    { method: "PATCH", body: JSON.stringify({ action }) },
    token,
  );
}

/** Retires a revoked runner's record. 409 UZ-RUN-016 if it is not revoked yet. */
export async function deleteRunner(token: string, runnerId: string): Promise<void> {
  await request<void>(`${FLEET_RUNNERS_PATH}/${encodeURIComponent(runnerId)}`, { method: "DELETE" }, token);
}

export async function listRunnerEvents(
  token: string,
  runnerId: string,
  params: EventListParams = {},
): Promise<RunnerEventsResponse> {
  const qs = keysetParams(params);
  if (params.event_type) qs.set("event_type", params.event_type);
  if (params.since !== undefined) qs.set("since", String(params.since));
  if (params.until !== undefined) qs.set("until", String(params.until));
  const suffix = qs.size > 0 ? `?${qs.toString()}` : "";
  return request<RunnerEventsResponse>(
    `${FLEET_RUNNERS_PATH}/${encodeURIComponent(runnerId)}/events${suffix}`,
    { method: "GET" },
    token,
  );
}

/**
 * Split the free-form labels field (comma-separated) into a deduped, validated
 * set. Returns the first offending label as an error so the form can surface it;
 * an empty/whitespace-only input is a valid empty set.
 */
export function parseLabels(raw: string): { labels: string[]; error: string | null } {
  const parts = raw.split(",").map((s) => s.trim()).filter((s) => s.length > 0);
  const seen = new Set<string>();
  for (const p of parts) {
    if (!LABEL_REGEX.test(p)) {
      return { labels: [], error: `Label "${p}" must be 1–64 chars: letters, digits, dot, hyphen, underscore` };
    }
    seen.add(p);
  }
  return { labels: [...seen], error: null };
}
