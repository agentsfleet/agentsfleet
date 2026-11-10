
export type { RunnerLease, RunnerLeaseResponse } from "./runner-control-leases";

import { RUNNER_ADMIN_ACTION, RUNNER_ADMIN_STATE } from "./runner-control-types";
import type {
  AssignedPolicy,
  RunnerAdminAction,
  RunnerAdminState,
  RunnerEventType,
  SandboxTier,
} from "./runner-control-types";

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
// daemon's `QUERY_STARTING_AFTER` / `QUERY_LIMIT` (rustd/crates/foundation/af_core/src/paging/mod.rs).
export const QUERY_STARTING_AFTER = "starting_after";

export const QUERY_LIMIT = "limit";

/** Lease-list filter parameter: only leases held for this workspace. */
export const QUERY_WORKSPACE_ID = "workspace_id";

export const QUERY_FLEET = "fleet";





export interface RunnerListItem {
  id: string;
  host_id: string;
  sandbox_tier: SandboxTier;
  status: RunnerAdminState;
  liveness: RunnerLiveness;
  labels: string[];
  last_seen_at: string;
  created_at: string;
  /** The assignment this host must satisfy. */
  assigned_policy: AssignedPolicy;
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

// One check's verdict, the TypeScript shape of `SelftestCheck`
// (rustd/crates/protocol/af_wire/src/runner/selftest.rs). `agentsfleet-runner probe`
// prints the same `{name, ok, detail}` triple under `checks`, so an operator
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
  /** An operator's outstanding ask, an RFC 3339 instant; null when none is pending. The
   * daemon clears it on the beat that reports the matching verdict, so a
   * non-null value means "asked, not yet answered". */
  selftest_requested_at: string | null;
  /** When the verdict landed, an RFC 3339 instant; null until a first report. A runner may
   * hold a request with no result, or a result with no request (the startup
   * probe, which no operator asked for). */
  selftest_completed_at: string | null;
  /** The latest verdict; null means never self-tested, which the page renders
   * differently from "tested and reported no checks". */
  selftest: SelftestReport | null;
}

/** The mint response — `runner_token` is the raw `agt_r`, returned exactly once. */
export interface CreatedRunner {
  runner_id: string;
  runner_token: string;
  assigned_policy: AssignedPolicy;
}

export interface RunnerAdminStateUpdate {
  id: string;
  status: RunnerAdminState;
}

/** The self-test PATCH reply: the recorded REQUEST, never a verdict. The daemon
 * picks the ask up on its next heartbeat and answers on a later one, so the page
 * shows pending and ages it from `selftest_requested_at`. */
export interface RunnerSelftestRequest {
  id: string;
  status: RunnerAdminState;
  selftest_requested_at: string;
}

/** At least one edit field; both are applied atomically by the daemon. */
export type RunnerEdit = (
  | { assigned_policy: AssignedPolicy; labels?: string[] }
  | { assigned_policy?: AssignedPolicy; labels: string[] }
) & { action?: never };

/** Fields not requested by this edit are returned as null. */
export interface RunnerEditResponse {
  id: string;
  status: RunnerAdminState;
  assigned_policy: AssignedPolicy | null;
  labels: string[] | null;
  selftest_requested_at: string | null;
}

export interface RunnerEventItem {
  id: string;
  runner_id: string;
  event_type: RunnerEventType;
  occurred_at: string;
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
