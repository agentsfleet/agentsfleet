//! Wire paths, the runner token prefix, and the lease wire version.
//!
//! Single-sourced here for the same reason `protocol.zig` single-sources them:
//! the router and every client must spell a path identically, and a path built
//! by concatenation at two call sites is two paths that drift.

/// Collection root for runner enrollment and the runner self-plane.
pub const RUNNERS: &str = "/v1/runners";

/// Runner-token prefix. The wire contract for the machine principal — the daemon
/// mints and validates it, and the host checks it before the lease loop.
pub const RUNNER_TOKEN_PREFIX: &str = "agt_r";

/// `POST /v1/runners/me/heartbeats` — liveness, capability report, assignment.
pub const RUNNER_HEARTBEATS: &str = "/v1/runners/me/heartbeats";

/// `POST /v1/runners/me/leases` — long-poll for the next event.
pub const RUNNER_LEASES: &str = "/v1/runners/me/leases";

/// `POST /v1/runners/me/reports` — the terminal result for a lease.
pub const RUNNER_REPORTS: &str = "/v1/runners/me/reports";

/// `GET`/`POST /v1/runners/me/memory/{fleet_id}` — durable fleet memory.
///
/// Collection prefix; the caller appends the `{fleet_id}` segment.
pub const RUNNER_MEMORY: &str = "/v1/runners/me/memory";

/// Trailing segment of `POST /v1/runners/me/memory/{fleet_id}/recall` — a
/// search past the hydration window. Bare for the reason
/// [`LEASE_ACTIVITY_SUFFIX`] gives.
pub const RUNNER_MEMORY_RECALL_SUFFIX: &str = "recall";

/// `GET /v1/runners/me` — read-only self status, which does not bump liveness.
pub const RUNNER_SELF: &str = "/v1/runners/me";

/// `GET /v1/runners/me/bundles/{content_hash}` — Fleet Bundle snapshot download.
///
/// Collection prefix; the caller appends the `{content_hash}` segment.
pub const RUNNER_BUNDLES: &str = "/v1/runners/me/bundles";

/// `POST /v1/runners/me/credentials/mint` — on-demand credential mint.
pub const RUNNER_CREDENTIALS_MINT: &str = "/v1/runners/me/credentials/mint";

/// Trailing segment of the per-lease activity sub-resource.
///
/// A bare segment rather than a joined constant: `lease_id` is a path parameter,
/// so the full path is `{RUNNER_LEASES}/{lease_id}/{LEASE_ACTIVITY_SUFFIX}`.
pub const LEASE_ACTIVITY_SUFFIX: &str = "activity";

/// Trailing segment of the per-lease renewal sub-resource. See
/// [`LEASE_ACTIVITY_SUFFIX`] for why it is a bare segment.
pub const LEASE_RENEW_SUFFIX: &str = "renew";

/// Trailing segment of the per-lease tool-call records sub-resource. See
/// [`LEASE_ACTIVITY_SUFFIX`] for why it is a bare segment.
pub const LEASE_TOOL_CALLS_SUFFIX: &str = "tool-calls";

/// Trailing segment of the per-lease schedules sub-resource: the schedules of
/// the fleet the lease runs. See [`LEASE_ACTIVITY_SUFFIX`] for why it is a
/// bare segment.
pub const LEASE_SCHEDULES_SUFFIX: &str = "schedules";

/// Trailing segment of one schedule's runs: its history, and where a run is
/// created to fire it now. Bare, beneath `{LEASE_SCHEDULES_SUFFIX}/{schedule_id}`.
pub const SCHEDULE_RUNS_SUFFIX: &str = "runs";

/// Trailing segment of the per-lease messages sub-resource: a line said to
/// the event's thread before the answer. See [`LEASE_ACTIVITY_SUFFIX`].
pub const LEASE_MESSAGES_SUFFIX: &str = "messages";
