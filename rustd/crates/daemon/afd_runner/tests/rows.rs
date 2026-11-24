//! The rows these suites seed beyond what `afd_db::test_util` spells: a run
//! with the event it executes, a fleet's thread, and a placed run.
//!
//! One column list each, so a schema change edits this file and not every
//! suite. Every row is stamped by the database's `now()` unless the suite
//! backdates it.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use af_core::id::Uuid7;
use afd_db::test_util::{RunnerPolicy, TurnFacts, TurnSeed, mint_id};
use jiff::Timestamp;
use jiff_sqlx::ToSqlx as _;
use sqlx::PgConnection;

/// The assignment every seeded runner carries.
pub(crate) const POLICY: RunnerPolicy<'static> = RunnerPolicy {
    sandbox_tier: "dev_none",
    network_policy: "deny_all_egress",
};

/// The event every seeded run executes.
pub(crate) const EVENT: TurnFacts<'static> = TurnFacts {
    actor: "test",
    event_type: "chat",
    status: "received",
};

const INSERT_RUN: &str = "\
INSERT INTO thread.runs
  (id, runner_id, fleet_id, workspace_id, tenant_id, turn_id, turn_created_at,
   posture, provider, model, last_metered_at, fencing_token,
   run_expires_at, status, created_at, updated_at)
VALUES ($1, $2, $3, $4, $5, $6,
        coalesce($9::timestamptz, now()), 'platform', 'test', 'test', coalesce($9::timestamptz, now()), 1,
        $7::timestamptz, $8, coalesce($9::timestamptz, now()),
        coalesce($9::timestamptz, now()))";

/// Where a run lives and what it owes.
pub(crate) struct RunRow<'a> {
    /// The run's own id.
    pub(crate) id: &'a Uuid7,
    /// The runner holding it.
    pub(crate) runner: &'a Uuid7,
    /// The fleet it is assigned.
    pub(crate) fleet: &'a Uuid7,
    /// The fleet's workspace.
    pub(crate) workspace: &'a Uuid7,
    /// The workspace's tenant.
    pub(crate) tenant: &'a Uuid7,
    /// Its status, as stored.
    pub(crate) status: &'a str,
    /// When it lapses.
    pub(crate) expires_at: Timestamp,
    /// When it was made and last touched; `None` is the database's `now()`.
    pub(crate) stamped_at: Option<Timestamp>,
}

/// Seeds the event a run executes, then the run, and answers the event's
/// id, which the run names.
pub(crate) async fn payload(connection: &mut PgConnection, row: RunRow<'_>) -> Uuid7 {
    let event = mint_id();
    let mut seed = TurnSeed::new(row.fleet, row.workspace, &event, EVENT);
    if let Some(instant) = row.stamped_at {
        seed = seed.at(instant);
    }
    seed.insert(&mut *connection)
        .await
        .expect("the held event seeds");
    sqlx::query(INSERT_RUN)
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
        .expect("the run seeds");
    event
}

/// What a placed run holds, and whether it still runs.
pub(crate) struct PlacedRunRow<'a> {
    /// Its status, as stored.
    pub(crate) status: &'a str,
    /// When its run lapses.
    pub(crate) run_expires_at: Timestamp,
}

/// Seeds one run `runner` holds for a fleet the suite needs no row of, and
/// answers its id. No foreign key reaches a run's fleet or turn, so both are
/// opaque ids here: the run occupies a worker and nothing else.
pub(crate) async fn placed_run(
    connection: &mut PgConnection,
    (runner, workspace, tenant): (&Uuid7, &Uuid7, &Uuid7),
    row: PlacedRunRow<'_>,
) -> Uuid7 {
    let id = mint_id();
    sqlx::query(INSERT_RUN)
        .bind(&id)
        .bind(runner)
        .bind(mint_id())
        .bind(workspace)
        .bind(tenant)
        .bind(mint_id())
        .bind(row.run_expires_at.to_sqlx())
        .bind(row.status)
        .bind(None::<jiff_sqlx::Timestamp>)
        .execute(&mut *connection)
        .await
        .expect("the placed run seeds");
    id
}
