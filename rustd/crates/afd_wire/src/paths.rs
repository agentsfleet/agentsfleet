//! Wire paths, the runner token prefix, and the lease wire version.
//!
//! Single-sourced here because the router and every client must spell a path
//! identically, and a path built by concatenation at two call sites is two
//! paths that drift.
//!
//! Each route is composed with `concatcp!` from the segments below, so the
//! daemon's router, its published API document and the runner's client read
//! one spelling. A segment renamed here moves every route under it.

use const_format::concatcp;

/// Collection root for runner enrollment and the runner self-plane.
pub const RUNNERS: &str = "/v1/runners";

/// Runner-token prefix. The wire contract for the machine principal — the daemon
/// mints and validates it, and the host checks it before the lease loop.
pub const RUNNER_TOKEN_PREFIX: &str = "agt_r";

/// `GET /v1/runners/me` — read-only self status, which does not bump liveness.
pub const RUNNER_SELF: &str = concatcp!(RUNNERS, "/me");

/// `POST /v1/runners/me/heartbeats` — liveness, capability report, assignment.
pub const RUNNER_HEARTBEATS: &str = concatcp!(RUNNER_SELF, "/heartbeats");

/// `POST /v1/runners/me/leases` — long-poll for the next event.
pub const RUNNER_LEASES: &str = concatcp!(RUNNER_SELF, "/leases");

/// `POST /v1/runners/me/reports` — the terminal result for a lease.
pub const RUNNER_REPORTS: &str = concatcp!(RUNNER_SELF, "/reports");

/// `GET`/`POST /v1/runners/me/memory/{fleet_id}` — durable fleet memory.
///
/// Collection prefix; the caller appends the `{fleet_id}` segment.
pub const RUNNER_MEMORY: &str = concatcp!(RUNNER_SELF, "/memory");

/// Trailing segment of `POST /v1/runners/me/memory/{fleet_id}/recall` — a
/// search past the hydration window. Bare for the reason
/// [`LEASE_ACTIVITY_SUFFIX`] gives.
pub const RUNNER_MEMORY_RECALL_SUFFIX: &str = "recall";

/// `GET /v1/runners/me/bundles/{content_hash}` — Fleet Bundle snapshot download.
///
/// Collection prefix; the caller appends the `{content_hash}` segment.
pub const RUNNER_BUNDLES: &str = concatcp!(RUNNER_SELF, "/bundles");

/// `POST /v1/runners/me/credentials/mint` — on-demand credential mint.
pub const RUNNER_CREDENTIALS_MINT: &str = concatcp!(RUNNER_SELF, "/credentials/mint");

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

/// One held lease, the root every per-lease route sits under. The router
/// matches `{lease_id}`; the runner puts the lease's id in its place.
const LEASE: &str = concatcp!(RUNNER_LEASES, "/{lease_id}/");

/// One fleet's memory: `GET` hydrates it, `POST` captures into it.
pub const RUNNER_MEMORY_FLEET: &str = concatcp!(RUNNER_MEMORY, "/{fleet_id}");

/// `POST` — a search of one fleet's memory past the hydration window.
pub const RUNNER_MEMORY_RECALL: &str =
    concatcp!(RUNNER_MEMORY_FLEET, "/", RUNNER_MEMORY_RECALL_SUFFIX);

/// `GET` — one Fleet Bundle snapshot by content hash.
pub const RUNNER_BUNDLE: &str = concatcp!(RUNNER_BUNDLES, "/{content_hash}");

/// `POST` — live-tail frames for a held lease.
pub const LEASE_ACTIVITY: &str = concatcp!(LEASE, LEASE_ACTIVITY_SUFFIX);

/// `POST` — more time on a held lease.
pub const LEASE_RENEW: &str = concatcp!(LEASE, LEASE_RENEW_SUFFIX);

/// `POST` — finished calls' full records for a held lease.
pub const LEASE_TOOL_CALLS: &str = concatcp!(LEASE, LEASE_TOOL_CALLS_SUFFIX);

/// `GET`/`POST` — the leased fleet's schedules.
pub const LEASE_SCHEDULES: &str = concatcp!(LEASE, LEASE_SCHEDULES_SUFFIX);

/// `PATCH`/`DELETE` — one schedule the leased fleet made.
pub const LEASE_SCHEDULE: &str = concatcp!(LEASE_SCHEDULES, "/{schedule_id}");

/// `GET`/`POST` — one schedule's runs.
pub const LEASE_SCHEDULE_RUNS: &str = concatcp!(LEASE_SCHEDULE, "/", SCHEDULE_RUNS_SUFFIX);

/// `POST` — a line said to the leased event's thread.
pub const LEASE_MESSAGES: &str = concatcp!(LEASE, LEASE_MESSAGES_SUFFIX);
