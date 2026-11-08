//! What `api_runtime` may and may not do, proven by running the statements.
//!
//! # Why this file exists
//!
//! The purge once shipped a `DELETE FROM memory.entries` that
//! `api_runtime` has never been entitled to run, and no suite caught it. A test
//! that connects as the database owner bypasses grants entirely, so it stays
//! green whether or not the runtime role can reach the table at all.
//!
//! Every test here therefore meets the grants a request meets, and asserts on
//! what Postgres then does — never on `has_table_privilege`. The two can
//! disagree, and only one of them is what a request meets.
//!
//! # Statements, then the purge itself
//!
//! The purge runs two statements — the row lock and the conditional delete of
//! `fleet.fleets` — and every child, memory included, cascades from that row
//! (schema/2100 for memory). The first two tests drive them under
//! `SET ROLE api_runtime`: one that both run, one that memory refuses
//! `api_runtime` outright, which is why the purge leaves memory to the cascade. Neither proves `Fleets::purge_tagged` works, because each runs the
//! statements itself.
//!
//! So the last test calls the real `purge` through the lane's API pool, which
//! authenticates as production's API login does — a member of `api_runtime`
//! and nothing else — over a fleet holding a row in every child table, memory
//! included, and counts each after.
//!
//! `#[ignore]`d; `make test-integration-rustd` runs it.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_fleet_lifecycle::FleetStatus;
use afd_fleet_lifecycle::purge_statements as statements;

use crate::integration_purge_ledger_identity::{
    SEEDED, rows_for, seed_everything_the_purge_destroys,
};
use crate::support::{Lane, mint};

/// A delete on memory, which `api_runtime` must be refused outright.
const MEMORY_DELETE: &str = "DELETE FROM memory.entries WHERE fleet_id = $1::uuid";

/// Takes the runtime role, so what follows meets the grants a request meets.
async fn as_api_runtime(connection: &mut sqlx::PgConnection) {
    sqlx::query("SET ROLE api_runtime")
        .execute(&mut *connection)
        .await
        .expect("the lane's user must be entitled to assume api_runtime");
}

/// The regression: the purge's own statements, in order, under the real role.
///
/// `FOR UPDATE` needs the UPDATE grant and the delete needs DELETE on
/// `fleet.fleets` (schema/1500); withholding either fails here.
#[tokio::test]
#[ignore = "needs the lane's Postgres and Dragonfly"]
async fn the_purge_statements_all_run_as_api_runtime() {
    let lane = Lane::create().await;
    // A fresh id matches nothing: the assertion is about the refusal, and a
    // delete entitled to run answers `DELETE 0` as happily as `DELETE 3`.
    let fleet = mint();
    let mut connection = lane.connection().await;

    sqlx::query("BEGIN")
        .execute(&mut *connection)
        .await
        .expect("the probe transaction must open");
    as_api_runtime(&mut connection).await;

    let locked = sqlx::query(statements::LOCK_FLEET)
        .bind(fleet.as_str())
        .bind(lane.workspace.as_str())
        .execute(&mut *connection)
        .await;
    assert!(
        locked.is_ok(),
        "api_runtime must hold UPDATE on fleet.fleets to lock it: {:?}",
        locked.err()
    );
    let deleted = sqlx::query(statements::DELETE_FLEET_IN_STATUS)
        .bind(fleet.as_str())
        .bind(lane.workspace.as_str())
        .bind(FleetStatus::Killed.as_str())
        .execute(&mut *connection)
        .await;
    assert!(
        deleted.is_ok(),
        "api_runtime must hold DELETE on fleet.fleets: {:?}",
        deleted.err()
    );

    sqlx::query("ROLLBACK")
        .execute(&mut *connection)
        .await
        .expect("the probe transaction must roll back");
    lane.cleanup().await;
}

/// The fence itself, asserted in the refusing direction.
///
/// Without this, a future `GRANT ... TO api_runtime` — or a login role handed
/// `pg_write_all_data`, which is exactly what hid the bug above — would let the
/// last test pass without proving the cascade crosses the fence.
#[tokio::test]
#[ignore = "needs the lane's Postgres and Dragonfly"]
async fn memory_stays_out_of_reach_until_the_role_is_assumed() {
    let lane = Lane::create().await;
    let fleet = mint();
    let mut connection = lane.connection().await;

    sqlx::query("BEGIN")
        .execute(&mut *connection)
        .await
        .expect("the probe transaction must open");
    as_api_runtime(&mut connection).await;

    let refused = sqlx::query(MEMORY_DELETE)
        .bind(fleet.as_str())
        .execute(&mut *connection)
        .await;

    let failure = refused.expect_err(
        "api_runtime reached memory without assuming memory_runtime — the fence \
         in schema/1010 is gone, or the login role holds pg_write_all_data",
    );
    let text = failure.to_string();
    assert!(
        text.contains("permission denied"),
        "the refusal must be a privilege refusal, not something else: {text}"
    );

    sqlx::query("ROLLBACK")
        .execute(&mut *connection)
        .await
        .expect("the probe transaction must roll back");
    lane.cleanup().await;
}

/// The regression, bound to the code that shipped it.
///
/// The two tests above prove the statements and the fence; neither proves that
/// `Fleets::purge_tagged` empties every child, because both drive the statements
/// themselves. This one calls the real purge through the lane's API pool,
/// which holds only `api_runtime`, over a row in every child table, so a schema that withheld
/// either DELETE, or a memory cascade the fence stopped, fails here and nowhere
/// else.
#[tokio::test]
#[ignore = "needs the lane's Postgres and Dragonfly"]
async fn the_purge_itself_runs_as_api_runtime() {
    let lane = Lane::create().await;
    let fleet = lane.fleet().await;
    seed_everything_the_purge_destroys(&lane, &fleet).await;
    let tag = lane.kill(&fleet).await;

    lane.fleets
        .purge_tagged(&lane.workspace, &fleet, &tag)
        .await
        .expect(
            "a pool holding only api_runtime must be able to purge: the memory \
             rows go by the fleet row's cascade, and the gate and session rows \
             need the DELETE grant on fleet.fleets",
        );

    assert_eq!(lane.fleet_count(&lane.workspace).await, 0);
    for (table, _seeded) in SEEDED {
        assert_eq!(
            rows_for(&lane, table, &fleet).await,
            0,
            "a purge as api_runtime left rows in {table}"
        );
    }
    lane.cleanup().await;
}
