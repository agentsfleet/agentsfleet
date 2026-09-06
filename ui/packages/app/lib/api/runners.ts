import { request } from "./client";

import { RUNNER_ADMIN_ACTION, RUNNER_ADMIN_STATE } from "./runners-types";
import type {
  AssignedPolicy,
  LeaseOutcome,
  RunnerAdminAction,
  RunnerAdminState,
  RunnerEventType,
  SandboxTier,
} from "./runners-types";

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

export const RUNNER_ADMIN_STATES = [
  RUNNER_ADMIN_STATE.active,
  RUNNER_ADMIN_STATE.cordoned,
  RUNNER_ADMIN_STATE.draining,
  RUNNER_ADMIN_STATE.drained,
  RUNNER_ADMIN_STATE.revoked,
] as const;

// Canonical Stripe-style paging parameter names — spelled identically to the
// daemon's `QUERY_STARTING_AFTER` / `QUERY_LIMIT` (http/pagination.zig).
export const QUERY_STARTING_AFTER = "starting_after";

export const QUERY_LIMIT = "limit";

/** Lease-list filter parameter: only leases held for this workspace. */
export const QUERY_WORKSPACE_ID = "workspace_id";

export const QUERY_FLEET = "fleet";

const FLEET_RUNNERS_PATH = "/v1/fleets/runners";

const RUNNERS_ENROLLMENT_PATH = "/v1/runners";

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
  // `?? null` rather than `=== null`: a daemon older than these columns omits
  // the keys entirely, so the field arrives undefined and a strict null check
  // would fall through and dereference it.
  const report = runner.selftest ?? null;
  if (report === null) return false;
  const assigned = runner.assigned_policy ?? null;
  if (assigned === null) return true;
  return report.sandbox_tier !== assigned.sandbox_tier || report.network_policy !== assigned.network_policy;
}

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
