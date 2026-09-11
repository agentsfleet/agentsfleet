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
//! Every test here therefore takes `SET ROLE api_runtime` first and asserts on
//! what Postgres then does — never on `has_table_privilege`. The two can
//! disagree, and only one of them is what a request meets.
//!
//! # Why statements and not `Fleets::purge`
//!
//! `api_runtime` is `NOLOGIN`, so no pool can connect as it; the role is only
//! reachable by assuming it on a connection already open. `purge` acquires its
//! own connection and cannot be handed one, so the lane proves the statements it
//! runs, taken from the crate by name rather than retyped.
//!
//! `#[ignore]`d; `make test-integration-rustd` runs it.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_fleet_lifecycle::purge_statements as statements;

use crate::support::{Lane, mint};

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
