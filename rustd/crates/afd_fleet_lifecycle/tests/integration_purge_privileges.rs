//! What `api_runtime` may and may not do, proven by running the statements.
//!
//! # Why this file exists
//!
//! The purge once shipped a `DELETE FROM memory.memory_entries` that
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
//! The purge runs no statement on memory: `fk_memory_entries_fleet_id` cascades
//! from the fleet row (schema/820). The first two tests drive what it does run
//! under `SET ROLE api_runtime`: one that its `core` deletes all run, one that
//! memory refuses `api_runtime` outright, which is why the purge leaves memory
//! to the cascade. Neither proves `Fleets::purge` works, because each runs the
//! statements itself.
//!
//! So the last test calls the real `purge` through a pool authenticating as a
//! login role holding only `api_runtime`, over a fleet holding a row in every
//! child table, memory included, and counts each after. `api_runtime` is
//! `NOLOGIN` and cannot be connected as, and `purge` acquires its own connection
//! and cannot be handed one — a role of the test's own making is what closes
//! that.
//!
//! `#[ignore]`d; `make test-integration-rustd` runs it.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::sync::Arc;

use afd_crypto::entropy::Entropy;
use afd_crypto::secret::Kek;
use afd_db::config::DbRole;
use afd_db::test_util::TestDatabase;
use afd_fleet_lifecycle::Fleets;
use afd_fleet_lifecycle::purge_statements as statements;

use crate::integration_patch_visibility::installed;
use crate::integration_purge_ledger_identity::{
    SEEDED, kill, rows_for, seed_everything_the_purge_destroys,
};
use crate::support::{Lane, mint};

/// A login role holding `api_runtime` and nothing else, created by the test.
///
/// The lane connects as the database owner, which bypasses grants entirely —
/// the blind spot that let an ungranted purge ship. A role of our own is the
/// only way to make the suite meet what a request meets, because `api_runtime`
/// is `NOLOGIN` and cannot be connected as directly.
const PROBE: &str = "purge_probe";

/// Any valid key: the purge decrypts nothing, so this only has to be well
/// formed, and borrowing the lane's would mean widening its module for no gain.
const PROBE_KEK: [u8; 32] = [0x4d; 32];

/// A delete on memory, which `api_runtime` must be refused outright.
const MEMORY_DELETE: &str = "DELETE FROM memory.memory_entries WHERE fleet_id = $1::uuid";

/// Takes the runtime role, so what follows meets the grants a request meets.
async fn as_api_runtime(connection: &mut sqlx::PgConnection) {
    sqlx::query("SET ROLE api_runtime")
        .execute(&mut *connection)
        .await
        .expect("the lane's user must be entitled to assume api_runtime");
}

/// The regression: the purge's own statements, in order, under the real role.
///
/// Fails on the tree as it stood before schema/900, at both `core` deletes for
/// want of the grant.
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

    sqlx::query(statements::ALLOW_GATE_PURGE)
        .execute(&mut *connection)
        .await
        .expect("api_runtime must be able to open the append-only guard");

    for &statement in statements::PURGE_CHILDREN {
        let outcome = sqlx::query(statement)
            .bind(fleet.as_str())
            .execute(&mut *connection)
            .await;
        assert!(
            outcome.is_ok(),
            "api_runtime must hold DELETE for `{statement}`: {:?}",
            outcome.err()
        );
    }

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
         in schema/110 is gone, or the login role holds pg_write_all_data",
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

/// The lane's URL with the probe role's credentials in place of the owner's.
///
/// Everything after the userinfo is kept — host, database, and the query string
/// carrying `sslmode`, which a rebuilt URL would drop and a TLS lane would then
/// refuse.
fn probe_url(lane_url: &str) -> String {
    let (scheme, rest) = lane_url
        .split_once("://")
        .expect("the lane's URL carries a scheme");
    let tail = rest.split_once('@').map_or(rest, |(_, tail)| tail);
    format!("{scheme}://{PROBE}:{PROBE}@{tail}")
}

/// A `Fleets` whose pool authenticates as [`PROBE`] — the daemon's shape.
///
/// The role is created through the lane's owner connection, because creating it
/// is setup rather than the thing under test. `IF NOT EXISTS` plus the
/// duplicate-object arm because the lane's database is shared and two suites can
/// reach this at once.
async fn restricted(lane: &Lane) -> Fleets {
    let mut connection = lane.connection().await;
    for statement in [
        "DO $$ BEGIN \
           EXECUTE format('CREATE ROLE %I LOGIN PASSWORD %L', 'purge_probe', 'purge_probe'); \
         EXCEPTION WHEN duplicate_object OR unique_violation THEN NULL; END $$",
        "GRANT api_runtime TO purge_probe",
    ] {
        sqlx::query(statement)
            .execute(&mut *connection)
            .await
            .expect("the lane's owner may create and grant to a probe role");
    }
    drop(connection);

    // Pinned to one connection, which is all a single purge needs. The lane's
    // Postgres is shared and its own fault suites connect through a proxy on a
    // 400 ms budget: a second full-size pool warming beside them is enough
    // connection pressure to make those budgets miss, which is a failure in
    // their file and a cause in this one.
    let database = TestDatabase::shared();
    let pool = database
        .open(
            DbRole::Api,
            &[
                (DbRole::Api.url_knob(), &probe_url(&database.url())),
                ("DATABASE_POOL_SIZE", "1"),
                ("DATABASE_MIN_POOL_SIZE", "1"),
            ],
        )
        .await;
    Fleets::new(
        pool,
        lane.queue.clone(),
        Arc::new(Kek::from_bytes(PROBE_KEK)),
        Entropy::new(),
    )
}

/// The regression, bound to the code that shipped it.
///
/// The two tests above prove the statements and the fence; neither proves that
/// `Fleets::purge` empties every child, because both drive the statements
/// themselves. This one calls the real purge through a pool that holds only
/// `api_runtime`, over a row in every child table, so a schema that withheld
/// either DELETE, or a memory cascade the fence stopped, fails here and nowhere
/// else.
#[tokio::test]
#[ignore = "needs the lane's Postgres and Dragonfly"]
async fn the_purge_itself_runs_as_api_runtime() {
    let lane = Lane::create().await;
    let fleet = installed(&lane).await;
    seed_everything_the_purge_destroys(&lane, &fleet.id).await;
    kill(&lane, &fleet.id).await;

    restricted(&lane)
        .await
        .purge(&lane.workspace, &fleet.id)
        .await
        .expect(
            "a pool holding only api_runtime must be able to purge: the memory \
             rows go by the fleet row's cascade, and the gate and session rows \
             need the DELETE grants schema/900 makes",
        );

    assert_eq!(lane.fleet_count(&lane.workspace).await, 0);
    for (table, _seeded) in SEEDED {
        assert_eq!(
            rows_for(&lane, table, &fleet.id).await,
            0,
            "a purge as api_runtime left rows in {table}"
        );
    }
    lane.cleanup().await;
}
