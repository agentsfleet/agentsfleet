//! What `api_runtime` may and may not do, proven by running the statements.
//!
//! # Why this file exists
//!
//! `schema_privilege_test.zig` asserted this and was deleted with the Zig daemon
//! tree in `2f0021d1b`. Nothing replaced it, and in the fifteen days that
//! followed the purge shipped a `DELETE FROM memory.memory_entries` that
//! `api_runtime` has never been entitled to run. No suite caught it, for the
//! reason `9e247bd92` wrote down when it built the original: these tests connect
//! as the database owner, which bypasses grants entirely, so they stay green
//! whether or not the runtime role can reach the table at all.
//!
//! Every test here therefore meets the grants a request meets, and asserts on
//! what Postgres then does — never on `has_table_privilege`. The two can
//! disagree, and only one of them is what a request meets.
//!
//! # Statements, then the purge itself
//!
//! The first two tests drive the crate's own constants under `SET ROLE
//! api_runtime`: one that the purge's statements all run, one that memory still
//! refuses before the role is taken. Both prove the mechanism, and neither
//! proves `Fleets::purge` uses it — they order the statements themselves, so a
//! purge that dropped the role change would leave them green. That was true of
//! this file when it was written, and it is the same shape of gap that let the
//! bug ship.
//!
//! So the last test calls the real `purge` through a pool authenticating as a
//! login role holding only `api_runtime`. `api_runtime` is `NOLOGIN` and cannot
//! be connected as, and `purge` acquires its own connection and cannot be handed
//! one — a role of the test's own making is what closes that.
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
use afd_fleet_lifecycle::purge_statements as statements;
use afd_fleet_lifecycle::{Fleets, Patch, Requested};

use crate::integration_patch_visibility::installed;
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

/// A fleet id that matches nothing, so every statement below is a no-op.
///
/// The assertion is about the REFUSAL, not about rows — a delete entitled to run
/// answers `DELETE 0` just as happily as `DELETE 3`, and seeding rows would only
/// add a way for the test to fail for a reason it is not about.
fn absent_fleet() -> String {
    mint().as_str().to_owned()
}

/// Takes the runtime role, so what follows meets the grants a request meets.
async fn as_api_runtime(connection: &mut sqlx::PgConnection) {
    sqlx::query("SET ROLE api_runtime")
        .execute(&mut *connection)
        .await
        .expect("the lane's user must be entitled to assume api_runtime");
}

/// The regression: the purge's own statements, in order, under the real role.
///
/// Fails on the tree as it stood before schema/900 — at the memory delete for
/// want of the role, and then at both `core` deletes for want of the grant.
#[tokio::test]
#[ignore = "needs the lane's Postgres and Redis"]
async fn the_purge_statements_all_run_as_api_runtime() {
    let lane = Lane::create().await;
    let fleet = absent_fleet();
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

    sqlx::query(statements::ASSUME_MEMORY_ROLE)
        .execute(&mut *connection)
        .await
        .expect("api_runtime must be able to assume memory_runtime");

    sqlx::query(statements::PURGE_MEMORY)
        .bind(&fleet)
        .execute(&mut *connection)
        .await
        .expect("the memory delete must run once the role is held");

    sqlx::query(statements::RELEASE_ROLE)
        .execute(&mut *connection)
        .await
        .expect("the memory role must be releasable");

    for &statement in statements::PURGE_CHILDREN {
        let outcome = sqlx::query(statement)
            .bind(&fleet)
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
/// `pg_write_all_data`, which is exactly what hid the bug above — would make the
/// test before this one pass for the wrong reason and take the boundary with it.
#[tokio::test]
#[ignore = "needs the lane's Postgres and Redis"]
async fn memory_stays_out_of_reach_until_the_role_is_assumed() {
    let lane = Lane::create().await;
    let fleet = absent_fleet();
    let mut connection = lane.connection().await;

    sqlx::query("BEGIN")
        .execute(&mut *connection)
        .await
        .expect("the probe transaction must open");
    as_api_runtime(&mut connection).await;

    let refused = sqlx::query(statements::PURGE_MEMORY)
        .bind(&fleet)
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
/// `Fleets::purge` USES them, because both drive the constants themselves. This
/// one calls the real purge through a pool that holds only `api_runtime`, so a
/// purge that skipped the role — or a schema that withheld either DELETE — fails
/// here and nowhere else.
#[tokio::test]
#[ignore = "needs the lane's Postgres and Redis"]
async fn the_purge_itself_runs_as_api_runtime() {
    let lane = Lane::create().await;
    let fleet = installed(&lane).await;
    lane.fleets
        .patch(
            &lane.workspace,
            &fleet.id,
            &Patch {
                status: Some(Requested::Killed),
                ..Patch::default()
            },
            Lane::now(),
        )
        .await
        .expect("active to killed is legal");

    restricted(&lane)
        .await
        .purge(&lane.workspace, &fleet.id)
        .await
        .expect(
            "a pool holding only api_runtime must be able to purge: the memory \
             rows need SET ROLE, and the gate and session rows need the DELETE \
             grants schema/900 makes",
        );

    assert_eq!(lane.fleet_count(&lane.workspace).await, 0);
    lane.cleanup().await;
}
