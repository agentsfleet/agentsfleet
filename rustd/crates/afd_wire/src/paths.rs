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

/// `GET /v1/runners/me` — read-only self status, which does not bump liveness.
pub const RUNNER_SELF: &str = "/v1/runners/me";

/// `GET /v1/runners/me/bundles/{content_hash}` — Fleet Bundle snapshot download.
///
/// Collection prefix; the caller appends the `{content_hash}` segment.
pub const RUNNER_BUNDLES: &str = "/v1/runners/me/bundles";

/// `POST /v1/runners/me/credentials/mint` — on-demand credential mint.
pub const RUNNER_CREDENTIALS_MINT: &str = "/v1/runners/me/credentials/mint";

/// `GET /v1/fleets/runners` — the platform-admin operator-plane read.
pub const FLEET_RUNNERS: &str = "/v1/fleets/runners";

/// Trailing segment of the per-lease activity sub-resource.
///
/// A bare segment rather than a joined constant: `lease_id` is a path parameter,
/// so the full path is `{RUNNER_LEASES}/{lease_id}/{LEASE_ACTIVITY_SUFFIX}`.
pub const LEASE_ACTIVITY_SUFFIX: &str = "activity";

/// Trailing segment of the per-lease renewal sub-resource. See
/// [`LEASE_ACTIVITY_SUFFIX`] for why it is a bare segment.
pub const LEASE_RENEW_SUFFIX: &str = "renew";

/// The lease wire version this port speaks, and the only one it implements.
///
/// Asserted against the fixture manifest, so the number cannot drift from what
/// the Zig emitter recorded.
pub const LEASE_WIRE_VERSION_CURRENT: u16 = 2;
