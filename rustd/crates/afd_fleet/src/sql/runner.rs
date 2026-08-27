//! `fleet.runners` and `fleet.runner_events` — enrolment, liveness, verdict.
//!
//! Every statement here is runner-scoped: a runner principal authorises only
//! verbs about itself, so each carries `runner_id` in its predicate and can
//! never resolve another runner's row. Text is byte-identical to
//! `http/handlers/runner/sql.zig`; what differs is how the wide ones are bound.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use sqlx::Postgres;
use sqlx::postgres::PgArguments;
use sqlx::query::Query;

/// A statement bound and ready to execute.
///
/// Named once so the binder signatures below read as one line each rather than
/// as three lines of sqlx generics.
pub type Bound<'q> = Query<'q, Postgres, PgArguments>;

/// Enrol a runner and record its enrolment event atomically.
///
/// One statement, so an observer can never see a registered runner with no
/// audit row explaining where it came from. The operator's ASSIGNED policy
/// lands on the row here — the host never writes it.
pub const INSERT_RUNNER_WITH_EVENT: &str = "\
WITH inserted AS (
  INSERT INTO fleet.runners
  (id, host_id, token_hash, sandbox_tier, admin_state, labels, tenant_id,
   last_seen_at, created_at, updated_at, network_policy, registry_allowlist, worker_count,
   degraded, degraded_reason)
VALUES ($1::uuid, $2::text, $3::text, $4::text, $5::text, $6::jsonb, NULL, $7::bigint, $8::bigint, $8::bigint,
        $13::text, $14::jsonb, $15::int, $16::bool, $17::text)
  RETURNING id
)
INSERT INTO fleet.runner_events
  (id, runner_id, event_type, metadata, dedup_key, created_at)
SELECT $9::uuid, id, $10::text,
       jsonb_build_object($11::text, $2::text, $12::text, $4::text),
       NULL, $8::bigint
FROM inserted";

/// Everything [`INSERT_RUNNER_WITH_EVENT`] needs, by name.
///
/// # Why this one gets a struct and most statements do not
///
/// It takes SEVENTEEN positional parameters, `$8` is referenced four times, and
/// the `VALUES` list mentions `$13` after `$16`. The Zig call site passes a flat
/// seventeen-element tuple; it is correct, and confirming that it is correct
/// means counting arguments against the statement text. Two same-typed
/// arguments transposed — `host_id` and `token_hash` are both text, and one is
/// a CREDENTIAL — compiles clean and writes the wrong column.
///
/// sqlx's `.bind()` is positionally identical, and this workspace disables
/// sqlx's `macros` feature deliberately (see the workspace manifest), so there
/// is no compile-time query checking to fall back on. Naming the fields is what
/// replaces it: the `$n` order is written ONCE, here, beside the text it
/// orders, and a caller names fields instead of counting slots.
///
/// Statements taking four parameters or fewer bind at the call site, where four
/// arguments are verifiable at a glance (`M-SIMPLE-ABSTRACTIONS`: three
/// problems do not justify forty solutions).
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
    /// Administrative state the row opens in.
    pub admin_state: &'a str,
    /// Operator labels, already rendered as a JSON array.
    ///
    /// Text rather than a `serde_json::Value` because the statement casts
    /// `$6::jsonb` itself and sqlx's `json` feature is off in this workspace —
    /// binding the rendered text is what the Zig does, and it keeps the feature
    /// set unchanged.
    pub labels_json: &'a str,
    /// Liveness at enrolment — always [`super::LAST_SEEN_NEVER`], so the fleet
    /// read derives `registered` rather than a fabricated `online`.
    pub last_seen_at: i64,
    /// Creation instant, shared by the row and its enrolment event.
    pub now: UnixMillis,
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
    /// The reconciled verdict at enrolment.
    pub degraded: bool,
    /// Why it reads degraded, or `None` when it does not.
    pub degraded_reason: Option<&'a str>,
}

impl<'a> RegisterRow<'a> {
    /// Binds this row to [`INSERT_RUNNER_WITH_EVENT`], in `$n` order.
    ///
    /// The two metadata keys (`$11`, `$12`) are constants rather than caller
    /// data, so they are supplied here: seventeen binds, fifteen of which a
    /// caller could get wrong, and now none that a caller supplies positionally.
    pub fn bind(&'a self) -> Bound<'a> {
        let millis = self.now.as_millis();
        sqlx::query(INSERT_RUNNER_WITH_EVENT)
            .bind(self.runner_id.as_str())
            .bind(self.host_id)
            .bind(self.token_digest)
            .bind(self.sandbox_tier)
            .bind(self.admin_state)
            .bind(self.labels_json)
            .bind(self.last_seen_at)
            .bind(millis)
            .bind(self.event_id.as_str())
            .bind(self.event_type)
            .bind(super::meta::HOST_ID)
            .bind(super::meta::SANDBOX_TIER)
            .bind(self.network_policy)
            .bind(self.registry_allowlist_json)
            .bind(self.worker_count)
            .bind(self.degraded)
            .bind(self.degraded_reason)
    }
}

/// `GET /v1/runners/me`. Deliberately omits `token_hash` — the self read backs
/// the operator command line's `status`, and a credential must never
/// round-trip.
pub const SELECT_RUNNER_SELF: &str = "\
SELECT id::text, admin_state, host_id, sandbox_tier, last_seen_at,
       network_policy, registry_allowlist::text, worker_count,
       capability_report::text, degraded, degraded_reason, extra_binds::text
FROM fleet.runners WHERE id = $1::uuid";

/// Replace one live runner credential and append its audit event atomically.
///
/// The old digest stops resolving at the instant the event becomes visible.
/// A revoked row remains terminal and is returned without changing either
/// table, so rotation cannot reopen it through a second authority channel.
pub const ROTATE_RUNNER_TOKEN: &str = "\
WITH current_runner AS (
  SELECT id, admin_state FROM fleet.runners WHERE id = $1::uuid FOR UPDATE
), updated AS (
  UPDATE fleet.runners r
  SET token_hash = $2::text, updated_at = $3::bigint
  FROM current_runner c
  WHERE r.id = c.id AND c.admin_state <> $4::text
  RETURNING r.id
), event AS (
  INSERT INTO fleet.runner_events
    (id, runner_id, event_type, metadata, dedup_key, created_at)
  SELECT $5::uuid, id, $6::text, '{}'::jsonb, NULL, $3::bigint FROM updated
  RETURNING id
)
SELECT admin_state, EXISTS (SELECT 1 FROM updated) AS changed FROM current_runner";

/// The heartbeat's policy read.
///
/// Assignment, stored capability, and the prior verdict, so every beat
/// reconciles and carries the current truth back. `selftest_requested_at` rides
/// the same read: an outstanding operator request reaches the host on the next
/// beat without a second query or a second endpoint.
pub const SELECT_RUNNER_ASSIGNED_POLICY: &str = "\
SELECT sandbox_tier, network_policy, registry_allowlist::text, worker_count,
       degraded, degraded_reason, capability_report::text, selftest_requested_at,
       extra_binds::text
FROM fleet.runners WHERE id = $1::uuid";

/// Store a fresh capability report and the reconciled verdict in one write.
pub const UPDATE_RUNNER_CAPABILITY_AND_VERDICT: &str = "\
UPDATE fleet.runners
SET capability_report = $2::jsonb, capability_reported_at = $3::bigint,
    degraded = $4::bool, degraded_reason = $5::text, updated_at = $3::bigint
WHERE id = $1::uuid";

/// Re-reconcile against the stored report (no fresh report this beat); the
/// guard makes a steady state write nothing at all.
pub const UPDATE_RUNNER_VERDICT: &str = "\
UPDATE fleet.runners
SET degraded = $2::bool, degraded_reason = $3::text, updated_at = $4::bigint
WHERE id = $1::uuid
  AND (degraded IS DISTINCT FROM $2::bool OR degraded_reason IS DISTINCT FROM $3::text)";

/// Heartbeat: bump liveness, and emit a `runner_online` event only on a real
/// transition.
///
/// `FOR UPDATE` serialises concurrent heartbeats from the same host so the
/// pre-bump `last_seen_at` the event tests is the true previous value. The
/// trailing WHERE is what keeps the event stream quiet: an event lands only
/// when the runner was never seen, or was stale past the threshold — a steady
/// heartbeat writes liveness without writing history.
pub const HEARTBEAT_WITH_TRANSITION_EVENT: &str = "\
WITH locked AS (
  SELECT id, last_seen_at FROM fleet.runners WHERE id = $1::uuid FOR UPDATE
), bumped AS (
  UPDATE fleet.runners r
  SET last_seen_at = $2::bigint, updated_at = $2::bigint
  FROM locked
  WHERE r.id = locked.id
  RETURNING locked.last_seen_at
)
INSERT INTO fleet.runner_events
  (id, runner_id, event_type, metadata, dedup_key, created_at)
SELECT $3::uuid, $1::uuid, $4::text,
       jsonb_build_object($5::text, last_seen_at), NULL, $2::bigint
FROM bumped
WHERE last_seen_at = $6::bigint OR ($2::bigint - last_seen_at) > $7::bigint";

/// Store a reported self-test verdict and retire the request that asked for it,
/// in one write.
///
/// `selftest_requested_at = NULL` is what makes the request one-shot: the beat
/// that reports a verdict is the beat that clears the ask, so a runner cannot
/// re-run the probe every interval until an operator intervenes.
///
/// The clear is unconditional rather than matched against the request that
/// prompted it. A startup probe arrives with no request outstanding and writes
/// NULL over NULL; and if an operator re-requests while a verdict is in flight,
/// losing that request costs one click, whereas matching would need a request
/// token on the wire to gain it.
pub const UPDATE_RUNNER_SELFTEST: &str = "\
UPDATE fleet.runners
SET selftest_checks = $2::jsonb, selftest_all_ok = $3::bool,
    selftest_sandbox_tier = $4::text, selftest_network_policy = $5::text,
    selftest_completed_at = $6::bigint, selftest_requested_at = NULL,
    updated_at = $6::bigint
WHERE id = $1::uuid";

/// Liveness-only bump, for the paths that must not emit history.
///
/// The fallback when [`HEARTBEAT_WITH_TRANSITION_EVENT`] could not run — an
/// event identifier that would not mint, or a statement Postgres refused. A
/// beat that writes liveness without its transition event is a quieter loss
/// than a beat that writes nothing: the fleet read stays true, and only the
/// audit trail has a gap.
pub const TOUCH_RUNNER_LAST_SEEN: &str = "\
UPDATE fleet.runners SET last_seen_at = $2, updated_at = $2 WHERE id = $1::uuid";

/// Move one runner's administrative state and append its audit event.
///
/// The locked row, update, and event live in one statement. The final select
/// returns the prior state even for an idempotent request, which lets the
/// service distinguish a harmless repeat from a forbidden attempt to move a
/// revoked runner without a read-then-write race.
pub const TRANSITION_RUNNER_ADMIN_STATE: &str = "\
WITH current_state AS (
  SELECT id, admin_state AS from_admin_state
  FROM fleet.runners
  WHERE id = $1::uuid
  FOR UPDATE
), updated AS (
  UPDATE fleet.runners r
  SET admin_state = $2::text, updated_at = $3::bigint
  FROM current_state c
  WHERE r.id = c.id
    AND ($4::bool OR c.from_admin_state <> 'revoked')
    AND c.from_admin_state <> $2::text
  RETURNING r.id
), event AS (
  INSERT INTO fleet.runner_events
    (id, runner_id, event_type, metadata, dedup_key, created_at)
  SELECT $5::uuid, id, $6::text,
         jsonb_build_object($7::text, from_admin_state, $8::text, $2::text),
         NULL, $3::bigint
  FROM current_state
  WHERE EXISTS (SELECT 1 FROM updated)
)
SELECT c.from_admin_state, EXISTS (SELECT 1 FROM updated) AS changed
FROM current_state c";

/// The policy mutation's locked state and reported capability.
pub const SELECT_RUNNER_PATCH_STATE: &str = "\
SELECT admin_state, capability_report::text
FROM fleet.runners WHERE id = $1::uuid FOR UPDATE";

/// Re-assign policy, reconciled verdict, and audit event atomically.
///
/// The caller holds this row's lock in the surrounding transaction. The
/// distinctness guard makes an identical request write neither row nor event.
pub const PATCH_RUNNER_ASSIGNED_POLICY: &str = "\
WITH updated AS (
  UPDATE fleet.runners
  SET sandbox_tier = $2::text, network_policy = $3::text,
      registry_allowlist = $4::jsonb, worker_count = $5::int,
      updated_at = $6::bigint, degraded = $13::bool,
      degraded_reason = $14::text, extra_binds = $15::jsonb
  WHERE id = $1::uuid
    AND (sandbox_tier IS DISTINCT FROM $2::text
      OR network_policy IS DISTINCT FROM $3::text
      OR registry_allowlist IS DISTINCT FROM $4::jsonb
      OR worker_count IS DISTINCT FROM $5::int
      OR extra_binds IS DISTINCT FROM $15::jsonb)
  RETURNING id
), event AS (
  INSERT INTO fleet.runner_events
    (id, runner_id, event_type, metadata, dedup_key, created_at)
  SELECT $7::uuid, id, $8::text,
         jsonb_build_object($9::text, $2::text, $10::text, $3::text,
                            $11::text, $4::jsonb, $12::text, $5::int),
         NULL, $6::bigint
  FROM updated
)
SELECT id::text FROM updated";

/// Record one outstanding self-test ask while preserving revocation terminality.
///
/// Returning the locked row even when the guard refuses the update lets the
/// caller distinguish a missing runner from a revoked one without a racy
/// follow-up read.
pub const PATCH_RUNNER_SELFTEST_REQUEST: &str = "\
WITH current_state AS (
  SELECT id, admin_state FROM fleet.runners WHERE id = $1::uuid FOR UPDATE
), updated AS (
  UPDATE fleet.runners r
  SET selftest_requested_at = $2::bigint, updated_at = $2::bigint
  FROM current_state c
  WHERE r.id = c.id AND c.admin_state <> 'revoked'
  RETURNING r.id
)
SELECT c.admin_state, EXISTS (SELECT 1 FROM updated) AS changed
FROM current_state c";
