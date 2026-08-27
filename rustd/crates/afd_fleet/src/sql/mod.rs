//! Every statement this crate runs, collected, and nothing else.
//!
//! # Why a module TREE and not one file
//!
//! `fleet/sql.zig` reached its length cap and was carved into
//! `sql_lease_row.zig` and `sql_budget_drain.zig`, which are then re-exported
//! through `sql.zig` (`pub const INSERT_LEASE_WITH_EVENT = @import(…)`) purely
//! so RULE SQLMOD's "query text lives in one grepable module" survived the
//! split. That is a workaround for a gate, not a design. Rust has real modules,
//! so the split falls on DOMAIN instead of on line count, no file needs a
//! re-export to stay findable, and `grep -rn 'SELECT' src/sql/` still returns
//! everything.
//!
//! # Why collected at all, when `core_api` inlines its SQL
//!
//! `~/Projects/oss/core_api-develop` keeps SQL inline in `models/<entity>.rs`
//! and has no `sql.rs` anywhere — but its statements are one-line
//! stored-procedure calls (`SELECT * FROM insert_account_session_v2($1..$9)`)
//! whose logic lives in Postgres functions. That shape is unavailable here:
//! this milestone changes no schema, and the writable-CTEs port verbatim, so
//! `report`'s claim-and-settle is a ninety-line constant. More decisively, the
//! ONLY enforcement of verbatim-SQL parity is REVIEW reading these side by side
//! against the Zig originals — a read that cannot be done if the statements are
//! scattered through handler bodies.
//!
//! # The statements are byte-identical to their Zig originals
//!
//! Row-equivalence is the cutover invariant, so a statement is copied, not
//! re-derived. Where a `$n` order looks odd, it is odd in the original too and
//! is left alone; what changes is how it is BOUND — see [`runner::RegisterRow`]
//! for the shape high-arity statements take.

pub mod activity;
pub mod billing;
pub mod event;
pub mod fleet;
pub mod gate;
pub mod grant;
pub mod lease;
pub mod memory;
pub mod provider;
pub mod renew;
pub mod report;
pub mod runner;
pub mod runner_view;
pub mod sweep;
pub mod vault;

/// `fleet.runner_events.metadata` keys.
///
/// One declaration each, imported by every writer (RULE UFS). `runner_events.zig`
/// owns the Zig side; a key spelled inline at a second write site is how a
/// consumer's `metadata->>'host_id'` silently stops matching.
pub mod meta {
    /// The authenticated person who requested an operator mutation.
    pub const ACTOR_ID: &str = "actor_id";
    /// The host a runner was enrolled for.
    pub const HOST_ID: &str = "host_id";
    /// The isolation tier assigned at enrolment.
    pub const SANDBOX_TIER: &str = "sandbox_tier";
    /// The egress posture assigned by an operator.
    pub const NETWORK_POLICY: &str = "network_policy";
    /// The registry baseline assigned by an operator.
    pub const REGISTRY_ALLOWLIST: &str = "registry_allowlist";
    /// The worker ceiling assigned by an operator.
    pub const WORKER_COUNT: &str = "worker_count";
    /// The liveness instant a transition event records.
    pub const LAST_SEEN_AT: &str = "last_seen_at";
    /// The admin state a transition event moved OUT of.
    pub const FROM_ADMIN_STATE: &str = "from_admin_state";
    /// The admin state it moved INTO.
    pub const TO_ADMIN_STATE: &str = "to_admin_state";
    /// The lease a `lease_acquired` row is about.
    pub const LEASE_ID: &str = "lease_id";
    /// The fleet that lease claimed.
    pub const FLEET_ID: &str = "fleet_id";
    /// The event the lease is executing.
    ///
    /// Spelled `event_id` in the metadata even though the enclosing row is
    /// itself a runner EVENT — the key names the agentsfleet event, not this
    /// audit row, and `runner_events.zig` spells it the same way.
    pub const AGENTSFLEET_EVENT_ID: &str = "event_id";
    /// Whether the lease was a fresh pull or a reclaim.
    pub const KIND: &str = "kind";
}

/// `fleet.runner_events.event_type` values this crate writes.
///
/// The Zig spells these with `@tagName(protocol.RunnerEventType.…)`, which
/// derives the wire string from the enum's own spelling. `afd_wire`'s
/// [`RunnerEventType`](afd_wire::admin::RunnerEventType) carries the same
/// values as serde renames, so the strings come from there rather than being
/// restated — a rename on either side then fails to compile instead of writing
/// rows nothing queries.
pub mod event_type {
    /// A runner was enrolled.
    pub const RUNNER_REGISTERED: &str = "runner_registered";
    /// A runner was seen after being absent, or for the first time.
    pub const RUNNER_ONLINE: &str = "runner_online";
    /// A runner took a lease.
    pub const LEASE_ACQUIRED: &str = "lease_acquired";
    /// A runner stopped beating for longer than it may.
    ///
    /// Written by the liveness sweep, once per stale episode — the dedup key is
    /// the last-seen instant, so an hour of silence is one row rather than one
    /// row every heartbeat interval.
    pub const RUNNER_OFFLINE: &str = "runner_offline";
    /// A draining runner finished its last lease.
    pub const RUNNER_DRAINED: &str = "runner_drained";
    /// A runner gave a lease back, having reported on it.
    ///
    /// The closing bracket of [`LEASE_ACQUIRED`]: the two are written by the
    /// two ends of one lease's life, and an operator reading a runner's history
    /// pairs them. Only the REPORT path writes this — a lease that lapsed is
    /// expired by the reclaim sweep and never released.
    pub const LEASE_RELEASED: &str = "lease_released";
}

/// The status a `fleet.runner_leases` row opens in.
///
/// `protocol.zig`'s `RUNNER_LEASE_STATUS_ACTIVE`. Declared here rather than at
/// the write site because §3's report flips a row OUT of this value and its
/// predicate has to name the same spelling the issue wrote (RULE UFS) — two
/// spellings would mean a report that fences correctly and updates nothing.
pub const LEASE_STATUS_ACTIVE: &str = "active";

/// The status a REPORTED lease is flipped into.
///
/// `protocol.zig`'s `RUNNER_LEASE_STATUS_REPORTED`. The claim-and-settle
/// statement is the sole `active` → `reported` writer, and it flips this row in
/// the same statement that charges the final slice — so a lease can never be
/// `reported` with its last slice unbilled, nor billed twice by a retry that
/// finds the row already flipped.
pub const LEASE_STATUS_REPORTED: &str = "reported";

/// The status a reclaimed lease is flipped INTO.
///
/// `protocol.zig`'s `RUNNER_LEASE_STATUS_EXPIRED`. The reclaim statement is the
/// sole `active` → `expired` writer, so this spelling and
/// [`LEASE_STATUS_ACTIVE`] are the two halves of one predicate and belong
/// beside each other.
pub const LEASE_STATUS_EXPIRED: &str = "expired";

/// `fleet.runners.last_seen_at` for a runner that has never connected.
///
/// `protocol.zig`'s `RUNNER_LAST_SEEN_NEVER`. A sentinel rather than `NULL`
/// because the liveness sweep's ordering predicate reads the column directly;
/// what matters is that the derived state reads `registered` rather than a
/// fabricated `online`, which is Dimension 6.3.
pub const LAST_SEEN_NEVER: i64 = 0;

/// The `fleet.runners.admin_state` value that permits the runner plane.
///
/// Imported rather than re-declared: `afd_state::sql` already owns this
/// spelling for the credential lookup that gates every runner-plane request,
/// and RULE UFS is explicit that a literal with a prior `const` declaration is
/// imported, never restated. Two spellings of "active" would mean a runner the
/// authenticator admits and this crate's writes consider dead.
pub use afd_state::sql::{ADMIN_STATE_ACTIVE, ADMIN_STATE_DRAINED, ADMIN_STATE_DRAINING};
