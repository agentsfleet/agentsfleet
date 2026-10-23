//! The rows these suites seed beyond what `afd_db::test_util` spells: a lease
//! with the event it executes, a fleet's slot, and an authoring attempt.
//!
//! One column list each, so a schema change edits this file and not every
//! suite. Every row is stamped by the database's `now()` unless the suite
//! backdates it.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use afd_db::test_util::{EventKind, FleetEventSeed, RunnerPolicy, mint_id};
use jiff::Timestamp;
use jiff_sqlx::ToSqlx as _;
use sqlx::PgConnection;

/// The assignment every seeded runner carries.
pub(crate) const POLICY: RunnerPolicy<'static> = RunnerPolicy {
    sandbox_tier: "dev_none",
    network_policy: "deny_all_egress",
};

/// The event every seeded lease executes.
pub(crate) const EVENT: EventKind<'static> = EventKind {
    actor: "test",
    event_type: "chat",
    status: "received",
};

const INSERT_LEASE: &str = "\
INSERT INTO runner.leases
  (id, runner_id, fleet_id, workspace_id, tenant_id, fleet_event_id, event_created_at,
   posture, provider, model, last_metered_at, fencing_token,
   lease_expires_at, status, posted_message_count, created_at, updated_at)
VALUES ($1::uuid, $2::uuid, $3::uuid, $4::uuid, $5::uuid, $6::uuid,
        coalesce($9::timestamptz, now()), 'platform', 'test', 'test', coalesce($9::timestamptz, now()), 1,
        $7::timestamptz, $8, 0, coalesce($9::timestamptz, now()),
        coalesce($9::timestamptz, now()))";

const INSERT_SLOT: &str = "\
INSERT INTO runner.fleet_slots
  (fleet_id, last_runner_id, fencing_token, lease_expires_at, hold_expires_at,
   created_at, updated_at)
VALUES ($1::uuid, $2::uuid, 1, $3::timestamptz, $4::timestamptz,
        coalesce($5::timestamptz, now()), coalesce($5::timestamptz, now()))";

const INSERT_ATTEMPT: &str = "\
INSERT INTO authoring.attempts
  (id, lease_id, runner_id, workspace_id, tenant_id, user_id, draft_id,
   message_sequence, fencing_token, model_token_hash, model_snapshot, status,
   lease_expires_at, expires_at, reserved_nanos, usage_unknown, call_count,
   output_token_count, reported_runtime_ms, last_activity_sequence, created_at,
   updated_at)
VALUES ($1::uuid, $2::uuid, $3::uuid, $4::uuid, $5::uuid, $6::uuid, $6::uuid, 1, 1, $1::text,
        '{}', $7, $8::timestamptz, $8::timestamptz, $9, false, 0, 0, 0, 0, now(), now())";

/// Where a lease lives and what it owes.
pub(crate) struct LeaseRow<'a> {
    /// The lease's own id.
    pub(crate) id: &'a str,
    /// The runner holding it.
    pub(crate) runner: &'a str,
    /// The fleet it claims.
    pub(crate) fleet: &'a str,
    /// The fleet's workspace.
    pub(crate) workspace: &'a str,
    /// The workspace's tenant.
    pub(crate) tenant: &'a str,
    /// Its status, as stored.
    pub(crate) status: &'a str,
    /// When it lapses.
    pub(crate) expires_at: Timestamp,
    /// When it was made and last touched; `None` is the database's `now()`.
    pub(crate) stamped_at: Option<Timestamp>,
}

/// Seeds the event a lease executes, then the lease, and answers the event's
/// row id, which Postgres minted.
pub(crate) async fn lease(connection: &mut PgConnection, row: LeaseRow<'_>) -> String {
    let producer_key = format!("evt_{}", row.id);
    let mut seed = FleetEventSeed::new(row.fleet, row.workspace, &producer_key, EVENT);
    if let Some(instant) = row.stamped_at {
        seed = seed.at(instant);
    }
    let event = seed
        .insert(&mut *connection)
        .await
        .expect("the leased event seeds");
    sqlx::query(INSERT_LEASE)
        .bind(row.id)
        .bind(row.runner)
        .bind(row.fleet)
        .bind(row.workspace)
        .bind(row.tenant)
        .bind(&event)
        .bind(row.expires_at.to_sqlx())
        .bind(row.status)
        .bind(row.stamped_at.map(Timestamp::to_sqlx))
        .execute(&mut *connection)
        .await
        .expect("the lease seeds");
    event
}

/// Seeds a fleet's slot, last held by `runner`.
pub(crate) async fn slot(
    connection: &mut PgConnection,
    (fleet, runner): (&str, &str),
    lease_expires_at: Timestamp,
    hold_expires_at: Option<Timestamp>,
    stamped_at: Option<Timestamp>,
) {
    sqlx::query(INSERT_SLOT)
        .bind(fleet)
        .bind(runner)
        .bind(lease_expires_at.to_sqlx())
        .bind(hold_expires_at.map(Timestamp::to_sqlx))
        .bind(stamped_at.map(Timestamp::to_sqlx))
        .execute(&mut *connection)
        .await
        .expect("the slot seeds");
}

/// What an attempt reserved, and whether it still runs.
pub(crate) struct AttemptRow<'a> {
    /// Its status, as stored.
    pub(crate) status: &'a str,
    /// When its lease lapses.
    pub(crate) lease_expires_at: Timestamp,
    /// The money it holds against the wallet.
    pub(crate) reserved_nanos: i64,
}

/// Seeds one authoring attempt `runner` ran, and answers its id.
pub(crate) async fn attempt(
    connection: &mut PgConnection,
    (runner, workspace, tenant): (&str, &str, &str),
    row: AttemptRow<'_>,
) -> String {
    let id = mint_id();
    sqlx::query(INSERT_ATTEMPT)
        .bind(&id)
        .bind(mint_id())
        .bind(runner)
        .bind(workspace)
        .bind(tenant)
        .bind(mint_id())
        .bind(row.status)
        .bind(row.lease_expires_at.to_sqlx())
        .bind(row.reserved_nanos)
        .execute(&mut *connection)
        .await
        .expect("the authoring attempt seeds");
    id
}
