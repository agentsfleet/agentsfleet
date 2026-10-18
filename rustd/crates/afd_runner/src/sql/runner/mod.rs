//! `runner.runners` and `runner.events` — enrolment, liveness, verdict.
//!
//! Every statement here is runner-scoped: a runner principal authorises only
//! verbs about itself, so each carries `runner_id` in its predicate and can
//! never resolve another runner's row. Each is a `.sql` file beside this
//! module, schema-qualified, and stamps its lifecycle columns with `now()`.

use afd_core::id::Uuid7;
use afd_db::statement::Bound;
use jiff::Timestamp;
use jiff_sqlx::ToSqlx as _;

/// Enrol a runner and record its enrolment event atomically.
///
/// One statement, so an observer can never see a registered runner with no
/// audit row explaining where it came from. The operator's ASSIGNED policy
/// lands on the row here — the host never writes it. The enrolment event takes
/// the row's own `created_at`, so the two cannot disagree about when.
pub const INSERT_RUNNER_WITH_EVENT: &str = include_str!("insert_runner_with_event.sql");

/// The lease-type counter a freshly enrolled host starts at.
const FIRST_LEASE_VERSION: i64 = 0;

/// Everything [`INSERT_RUNNER_WITH_EVENT`] needs, by name.
///
/// # Why this one gets a struct and most statements do not
///
/// It takes sixteen positional parameters and `$2` and `$4` are each read
/// twice. A flat bind list is correct only as far as someone counts arguments
/// against the statement text. Two same-typed arguments transposed —
/// `host_id` and `token_hash` are both text, and one is a CREDENTIAL — compiles
/// clean and writes the wrong column.
///
/// sqlx's `.bind()` is positionally identical, and this workspace disables
/// sqlx's `macros` feature deliberately (see the workspace manifest), so there
/// is no compile-time query checking to fall back on. Naming the fields is what
/// replaces it: the `$n` order is written ONCE, here, beside the text it
/// orders, and a caller names fields instead of counting slots.
#[derive(Debug)]
pub struct RegisterRow<'a> {
    /// The runner's durable identifier.
    pub runner_id: &'a Uuid7,
    /// The host being enrolled.
    pub host_id: &'a str,
    /// SHA-256 hex of the minted token. The token itself is never stored.
    pub token_digest: &'a str,
    /// Assigned isolation tier, as its wire spelling.
    pub sandbox_tier: &'a str,
    /// Status the row opens in.
    pub status: &'a str,
    /// Operator labels, already rendered as a JSON array.
    ///
    /// Text rather than a `serde_json::Value` because the statement casts
    /// `$6::jsonb` itself and sqlx's `json` feature is off in this workspace;
    /// binding the rendered text keeps the feature set unchanged.
    pub labels_json: &'a str,
    /// Liveness at enrolment — always [`super::NEVER_SEEN`], so the fleet read
    /// derives `registered` rather than a fabricated `online`.
    pub last_seen_at: Timestamp,
    /// Identifier of the enrolment event row.
    pub event_id: &'a Uuid7,
    /// The event type that row records.
    pub event_type: &'a str,
    /// Assigned egress posture, as its wire spelling.
    pub network_policy: &'a str,
    /// Assigned registry baseline, already rendered as a JSON array.
    pub registry_allowlist_json: &'a str,
    /// Assigned worker ceiling, already clamped.
    pub worker_count: i32,
    /// Why the reconciled verdict reads degraded, or `None` when it does not.
    pub degraded_reason: Option<&'a str>,
}

impl<'a> RegisterRow<'a> {
    /// Binds this row to [`INSERT_RUNNER_WITH_EVENT`], in `$n` order.
    ///
    /// The two metadata keys (`$15`, `$16`) and the opening lease version
    /// (`$12`) are constants rather than caller data, so they are supplied
    /// here and no caller supplies anything positionally.
    pub fn bind(&'a self) -> Bound<'a> {
        sqlx::query(INSERT_RUNNER_WITH_EVENT)
            .bind(self.runner_id.as_str())
            .bind(self.host_id)
            .bind(self.token_digest)
            .bind(self.sandbox_tier)
            .bind(self.status)
            .bind(self.labels_json)
            .bind(self.network_policy)
            .bind(self.registry_allowlist_json)
            .bind(self.worker_count)
            .bind(self.degraded_reason)
            .bind(self.last_seen_at.to_sqlx())
            .bind(FIRST_LEASE_VERSION)
            .bind(self.event_id.as_str())
            .bind(self.event_type)
            .bind(super::meta::HOST_ID)
            .bind(super::meta::SANDBOX_TIER)
    }
}

/// `GET /v1/runners/me`. Deliberately omits `token_hash` — the self read backs
/// the operator command line's `status`, and a credential must never
/// round-trip.
pub const SELECT_RUNNER_SELF: &str = include_str!("select_runner_self.sql");

/// Replace one live runner credential and append its audit event atomically.
///
/// The old digest stops resolving at the instant the event becomes visible.
/// A revoked row remains terminal and is returned without changing either
/// table, so rotation cannot reopen it through a second authority channel.
/// `$1` runner, `$2` the new digest, `$3` the revoked status, `$4` event id,
/// `$5` event type, `$6` the actor key, `$7` the actor.
pub const ROTATE_RUNNER_TOKEN: &str = include_str!("rotate_runner_token.sql");

/// The heartbeat's policy read.
///
/// Assignment, stored capability, and the prior verdict, so every beat
/// reconciles and carries the current truth back. Whether a self-test is
/// outstanding rides the same read: an operator's request reaches the host on
/// the next beat without a second query or a second endpoint.
pub const SELECT_RUNNER_ASSIGNED_POLICY: &str = include_str!("select_runner_assigned_policy.sql");

/// Store a fresh capability report and the reconciled verdict in one write.
pub const UPDATE_RUNNER_CAPABILITY_AND_VERDICT: &str =
    include_str!("update_runner_capability_and_verdict.sql");

/// Re-reconcile against the stored report (no fresh report this beat); the
/// guard makes a steady state write nothing at all.
pub const UPDATE_RUNNER_VERDICT: &str = include_str!("update_runner_verdict.sql");

/// Heartbeat: bump liveness, and emit a `runner_online` event only on a real
/// transition.
///
/// `FOR UPDATE` serialises concurrent heartbeats from the same host so the
/// pre-bump `last_seen_at` the event tests is the true previous value. The
/// staleness test reads `clock_timestamp()` in a step that depends on the
/// locked row, so a beat that waited on the lock past the threshold is judged
/// at the instant it got the row, never at the instant it asked.
///
/// The trailing WHERE keeps the event stream quiet: an event lands only when
/// the runner was never seen, or was stale past the threshold. The metadata's
/// `last_seen_at` is the previous beat as RFC 3339 in UTC, trailing fraction
/// zeros trimmed — the form `jiff` writes, so the operator API serves one
/// spelling of an instant. `$1` runner, `$2` event id, `$3` event type, `$4`
/// the metadata key, `$5` the never-seen sentinel, `$6` the offline threshold.
pub const HEARTBEAT_WITH_TRANSITION_EVENT: &str =
    include_str!("heartbeat_with_transition_event.sql");

/// Store a reported self-test verdict and retire the request that asked for it,
/// in one write.
///
/// `selftest_requested_at = NULL` is what makes the request one-shot: the beat
/// that reports a verdict is the beat that clears the ask, so a runner cannot
/// re-run the probe every interval until an operator intervenes. Whether it
/// passed is not stored: it is read back from `selftest_checks`, which the
/// acceptor has already held to the host's own summary.
pub const UPDATE_RUNNER_SELFTEST: &str = include_str!("update_runner_selftest.sql");

/// Liveness-only bump, for the paths that must not emit history.
///
/// The fallback when [`HEARTBEAT_WITH_TRANSITION_EVENT`] could not run — an
/// event identifier that would not mint, or a statement Postgres refused. A
/// beat that writes liveness without its transition event is a quieter loss
/// than a beat that writes nothing: the fleet read stays true, and only the
/// audit trail has a gap.
pub const TOUCH_RUNNER_LAST_SEEN: &str = include_str!("touch_runner_last_seen.sql");
