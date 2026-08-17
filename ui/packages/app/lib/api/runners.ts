import { request } from "./client";

// host_id is free-form but bounded by the backend; deriving HOST_ID_REGEX from
// HOST_ID_MAX keeps the form in step with `register.zig`'s MAX_HOST_ID_LEN as a
// single source — the bound lives in exactly one place.
export const HOST_ID_MAX = 256;
export const HOST_ID_REGEX = new RegExp(`^[A-Za-z0-9_.-]{1,${HOST_ID_MAX}}$`);
export const LABEL_REGEX = /^[A-Za-z0-9_.-]{1,64}$/;

// Assignable isolation strength — mirrors `protocol.SandboxTier` verbatim
// (UFS: the tag names are the wire shape). Only tiers with real enforcement
// are assignable (the Seatbelt tier was removed — it never had enforcement
// code, and a tier that cannot be applied must not be assignable). `dev_none`
// is dev-only; a release daemon refuses it at boot.
export const SANDBOX_TIERS = ["landlock_full", "container_nested", "dev_none"] as const;
export type SandboxTier = (typeof SANDBOX_TIERS)[number];

// Operator-facing labels for the assignable isolation tiers — the raw enum
// tags are the wire shape (sent verbatim), these are the human strings the
// dropdown and list render. Keyed so a new tier can't be added without a label.
export const SANDBOX_TIER_LABELS: Record<SandboxTier, string> = {
  landlock_full: "Linux · Landlock (full)",
  container_nested: "Nested container",
  dev_none: "None (dev only)",
};

// One-line descriptions for the isolation-mode OptionCard picker. Keyed by
// the same SandboxTier so a new tier can't be added without one. Mirrors
// docs/architecture/runner_fleet.md §Sandbox tiers — do not restate from the
// label alone; that table is the source of truth for what each tier means.
export const SANDBOX_TIER_DESCRIPTIONS: Record<SandboxTier, string> = {
  landlock_full: "Bare Linux host with kernel-level Landlock sandboxing — full isolation, eligible for any work.",
  container_nested: "Runner runs inside a container on a Linux host or VM — same full-isolation tier as Landlock.",
  dev_none: "No real sandbox — for local development builds only; a non-debug runner build refuses to start with this tier.",
};

// Egress posture assigned per runner — mirrors `protocol.NetworkPolicy` verbatim
// (UFS: the tag names are the wire shape). `allow_list_egress` marks the runner
// degraded until its enforcement ships; the dialog says so when offering it.
export const NETWORK_POLICIES = ["allow_all", "deny_all_egress", "allow_list_egress"] as const;
export type NetworkPolicy = (typeof NETWORK_POLICIES)[number];

// Operator-facing labels for the egress postures — the raw tags are the wire
// shape, these are the strings the select renders. Keyed so a new mode can't
// be added without a label.
export const NETWORK_POLICY_LABELS: Record<NetworkPolicy, string> = {
  allow_all: "Allow all egress",
  deny_all_egress: "No egress",
  allow_list_egress: "Allowlist egress",
};

// One-line descriptions for the egress select. `allow_list_egress` says
// exactly what assigning it does today: the host cannot enforce it yet, so the
// runner reads degraded and refuses work until that enforcement ships.
export const NETWORK_POLICY_DESCRIPTIONS: Record<NetworkPolicy, string> = {
  allow_all: "All outbound traffic allowed — the interim open posture.",
  deny_all_egress: "No outbound network at all.",
  allow_list_egress: "Outbound only to an approved list. Enforcement has not shipped yet — a runner assigned this is degraded and receives no work until it does.",
};

// Enrollment defaults for the policy fields. Network defaults to the explicit
// interim open posture — defaulting to the strict allowlist before its
// enforcement ships would degrade every new runner. `DEFAULT_WORKER_COUNT`,
// `MIN_WORKER_COUNT`, and `MAX_WORKER_COUNT` mirror their `protocol.*`
// namesakes (UFS cross-runtime names); the server clamps into the same bounds.
export const DEFAULT_ASSIGNED_NETWORK_POLICY: NetworkPolicy = "allow_all";
export const DEFAULT_WORKER_COUNT = 1;
export const MIN_WORKER_COUNT = 1;
export const MAX_WORKER_COUNT = 64;

// Registry allowlist entries are host[:port] names, one per comma. The server
// enforces the same grammar and cap (`protocol.MAX_REGISTRY_ENTRIES`, UFS
// cross-runtime name) — the dialog refuses first so the operator hears it
// in-form rather than as a 400.
export const REGISTRY_HOST_REGEX = /^[A-Za-z0-9_.-]{1,253}(:[0-9]{1,5})?$/;
export const MAX_REGISTRY_ENTRIES = 32;

/**
 * Split the free-form registry allowlist field (comma-separated) into a
 * deduped, validated set — the registry twin of `parseLabels`. An
 * empty/whitespace-only input is a valid empty set (the runner substitutes its
 * default registry set).
 */
export function parseRegistryAllowlist(raw: string): { hosts: string[]; error: string | null } {
  const parts = raw.split(",").map((s) => s.trim()).filter((s) => s.length > 0);
  const seen = new Set<string>();
  for (const p of parts) {
    if (!REGISTRY_HOST_REGEX.test(p)) {
      return { hosts: [], error: `Registry "${p}" must be a host name, optionally with a port` };
    }
    seen.add(p);
  }
  if (seen.size > MAX_REGISTRY_ENTRIES) {
    return { hosts: [], error: `At most ${MAX_REGISTRY_ENTRIES} registries per runner` };
  }
  return { hosts: [...seen], error: null };
}

// How an operator-added path is mounted — mirrors `protocol_bind.BindMode`.
// An entry that names no mode is read-only, so access never widens by omission.
export const BIND_MODE = {
  read_only: "read_only",
  read_write: "read_write",
} as const;
export type BindMode = (typeof BIND_MODE)[keyof typeof BIND_MODE];

// One operator-assigned sandbox mount — mirrors `protocol_bind.ExtraBind`.
// `read_write` is a real boundary widening: tenant agent code can then modify
// host state outside its workspace on every lease that runner takes, so the
// page renders it differently from a plain row.
export interface ExtraBind {
  path: string;
  mode?: BindMode;
  note?: string;
}

// The policy the operator assigns to a runner — mirrors `protocol.AssignedPolicy`
// verbatim. The host applies exactly this; it never declares its own.
export interface AssignedPolicy {
  sandbox_tier: SandboxTier;
  network_policy: NetworkPolicy;
  registry_allowlist: string[];
  worker_count: number;
  // Paths bound IN ADDITION to the daemon-owned baseline. Optional on the wire:
  // a runner enrolled before the column existed sends nothing, which reads as
  // "baseline only" rather than as a missing assignment.
  extra_binds?: ExtraBind[];
}

// What the host's kernel can actually enforce — mirrors
// `protocol.CapabilityReport` verbatim (UFS: field names are the wire shape).
// Unauthenticated self-assertion; the server reconciles it against the
// assignment into `degraded`, and the dashboard only ever renders it beside
// the assignment it was judged against.
export interface CapabilityReport {
  landlock: boolean;
  seccomp: boolean;
  cgroup_controllers: string[];
  bubblewrap: boolean;
  egress_enforcement: boolean;
}

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

// The PATCH verbs the daemon serves — mirrors `protocol.RunnerAdminAction`.
export const RUNNER_ADMIN_ACTION = {
  cordon: "cordon",
  drain: "drain",
  revoke: "revoke",
  self_test: "self_test",
} as const;
export type RunnerAdminAction = (typeof RUNNER_ADMIN_ACTION)[keyof typeof RUNNER_ADMIN_ACTION];

// The subset that moves `admin_state`. `self_test` records a request and moves
// nothing, so the transition map and `actionsFor` key on THIS narrower type —
// same reasoning that keeps Delete out of ACTION_CONFIG. Widening the map to
// every wire verb would make a transition table answer for a non-transition.
export const RUNNER_ADMIN_ACTIONS = [
  RUNNER_ADMIN_ACTION.cordon,
  RUNNER_ADMIN_ACTION.drain,
  RUNNER_ADMIN_ACTION.revoke,
] as const;
export type RunnerStateAction = (typeof RUNNER_ADMIN_ACTIONS)[number];

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
  "runner_policy_assigned",
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
  "runner_policy_assigned",
] as const satisfies readonly RunnerEventType[];

// Canonical Stripe-style paging parameter names — spelled identically to the
// daemon's `QUERY_STARTING_AFTER` / `QUERY_LIMIT` (http/pagination.zig).
export const QUERY_STARTING_AFTER = "starting_after";
export const QUERY_LIMIT = "limit";
/** Lease-list filter parameter: only leases held for this workspace. */
export const QUERY_WORKSPACE_ID = "workspace_id";
export const QUERY_FLEET = "fleet";

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
  /** The assignment this host must satisfy. Null only for a pre-policy row —
   * such a runner reads degraded until an operator assigns a policy. */
  assigned_policy: AssignedPolicy | null;
  /** The host's last capability report; null until its first report arrives. */
  achievable: CapabilityReport | null;
  /** Assigned exceeds achievable (or no report yet). A degraded runner is issued no work. */
  degraded: boolean;
  /** The specific missing mechanism; null when not degraded. */
  degraded_reason: string | null;
}

export interface RunnerListResponse {
  items: RunnerListItem[];
  total: number | null;
  next_cursor: string | null;
}

// One check's verdict — mirrors `protocol_selftest.SelftestCheck`. The same
// `{name, ok, detail}` triple `agentsfleet-runner doctor` speaks, so an operator
// reads one vocabulary across both surfaces. `detail` is prose even when `ok`.
export interface SelftestCheck {
  name: string;
  ok: boolean;
  detail: string;
}

// One probe run — mirrors `protocol_selftest.SelftestReport`. The tier and
// policy travel WITH the verdict rather than being read live at render time: a
// result outlives the assignment that produced it, and rendering an old verdict
// against a new policy would tell an operator their policy is proven when
// nothing has tested it.
export interface SelftestReport {
  checks: SelftestCheck[];
  all_ok: boolean;
  sandbox_tier: string;
  network_policy: string;
}

/** The single-runner operator read: the list fields plus live-work and lifetime counters. */
export interface RunnerDetail extends RunnerListItem {
  active_lease_count: number;
  active_fleet_count: number;
  leases_acquired: number;
  leases_succeeded: number;
  leases_failed: number;
  leases_expired: number;
  /** An operator's outstanding ask, epoch ms; null when none is pending. The
   * daemon clears it on the beat that reports the matching verdict, so a
   * non-null value means "asked, not yet answered". */
  selftest_requested_at: number | null;
  /** When the verdict landed, epoch ms; null until a first report. A runner may
   * hold a request with no result, or a result with no request (the startup
   * probe, which no operator asked for). */
  selftest_completed_at: number | null;
  /** The latest verdict; null means never self-tested, which the page renders
   * differently from "tested and reported no checks". */
  selftest: SelftestReport | null;
}

/** True when a verdict describes an assignment the runner no longer carries.
 * The result is then history, not a statement about how this runner is
 * configured now, and the page must say so (Dimension 1.3). */
export function isSelftestStale(runner: RunnerDetail): boolean {
  const report = runner.selftest;
  if (report === null) return false;
  const assigned = runner.assigned_policy;
  if (assigned === null) return true;
  return report.sandbox_tier !== assigned.sandbox_tier || report.network_policy !== assigned.network_policy;
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
  assigned_policy: AssignedPolicy;
}

export interface RunnerAdminStateUpdate {
  id: string;
  admin_state: RunnerAdminState;
}

/** The self-test PATCH reply: the recorded REQUEST, never a verdict. The daemon
 * picks the ask up on its next heartbeat and answers on a later one, so the page
 * shows pending and ages it from `selftest_requested_at`. */
export interface RunnerSelftestRequest {
  id: string;
  admin_state: RunnerAdminState;
  selftest_requested_at: number;
}

/** The policy-update PATCH reply: the assignment as stored (worker count clamped). */
export interface RunnerPolicyUpdate {
  id: string;
  admin_state: RunnerAdminState;
  assigned_policy: AssignedPolicy;
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
  /** When set, the page holds only leases for this workspace. */
  workspace_id?: string;
  /**
   * When set, the page holds only leases for this fleet, named by its id or its
   * exact name. Intersects with `workspace_id` rather than replacing it.
   */
  fleet?: string;
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
  if (params.workspace_id) qs.set(QUERY_WORKSPACE_ID, params.workspace_id);
  if (params.fleet) qs.set(QUERY_FLEET, params.fleet);
  const suffix = qs.size > 0 ? `?${qs.toString()}` : "";
  return request<RunnerLeaseResponse>(
    `${FLEET_RUNNERS_PATH}/${encodeURIComponent(runnerId)}/leases${suffix}`,
    { method: "GET" },
    token,
  );
}

export async function createRunner(
  token: string,
  body: { host_id: string; assigned_policy: AssignedPolicy; labels: string[] },
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

/** Ask a runner to test its own sandbox. Returns once the request is recorded —
 * it does NOT wait for the verdict, because the daemon collects the ask on its
 * own heartbeat and waiting would hang the page on the offline host an operator
 * most wants to test. A revoked runner refuses (409 UZ-RUN-018). */
export async function requestRunnerSelftest(
  token: string,
  runnerId: string,
): Promise<RunnerSelftestRequest> {
  return request<RunnerSelftestRequest>(
    `${FLEET_RUNNERS_PATH}/${encodeURIComponent(runnerId)}`,
    { method: "PATCH", body: JSON.stringify({ action: RUNNER_ADMIN_ACTION.self_test }) },
    token,
  );
}

/** Re-assign a runner's policy. Reaches the host on its next heartbeat — no
 * host visit, no restart. Idempotent: a same-values PATCH changes nothing. */
export async function updateRunnerPolicy(
  token: string,
  runnerId: string,
  assigned_policy: AssignedPolicy,
): Promise<RunnerPolicyUpdate> {
  return request<RunnerPolicyUpdate>(
    `${FLEET_RUNNERS_PATH}/${encodeURIComponent(runnerId)}`,
    { method: "PATCH", body: JSON.stringify({ assigned_policy }) },
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
