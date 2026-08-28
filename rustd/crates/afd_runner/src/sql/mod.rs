//! Every statement the runner plane runs, and the stored vocabulary it writes.
//!
//! `fleet.runners` and `fleet.runner_events` are this crate's tables. The one
//! writer OUTSIDE it is `afd_fleet::lease::finalize`, which records a
//! `lease_released` runner-event when a lease ends — it reads [`meta`] and
//! [`event_type`] from here, because a key spelled inline at a second write
//! site is how a consumer's `metadata->>'lease_id'` silently stops matching.

pub use afd_state::sql::{
    ADMIN_STATE_ACTIVE, ADMIN_STATE_DRAINED, ADMIN_STATE_DRAINING, LAST_SEEN_NEVER,
    LEASE_STATUS_ACTIVE, LEASE_STATUS_EXPIRED, LEASE_STATUS_REPORTED,
};

pub mod runner;
pub mod runner_view;
pub mod sweep;

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
