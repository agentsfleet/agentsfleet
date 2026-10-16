//! Slot 955 against a populated database: a runner's assignment is required.
//!
//! Each proof migrates a scratch database through slot 954, writes runner rows,
//! then carries it forward with the production migrator.

use super::cutover_rows::through;
use super::*;
use afd_db::Db;
use afd_db::test_util::mint_id;

/// The slot this file proves.
const RUNNER_POLICY: i32 = 955;

/// The slot before it, which the proofs migrate through first.
const BEFORE: i32 = 954;

/// Postgres's `not_null_violation`.
const NOT_NULL_VIOLATION: &str = "23502";

/// A runner as enrolment writes it, assignment included.
const ASSIGNED: &str = "\
INSERT INTO fleet.runners (id, host_id, token_hash, sandbox_tier, admin_state, labels,
  network_policy, registry_allowlist, last_seen_at, created_at, updated_at)
VALUES ($1::uuid, $1, $1, 'dev_none', 'active', '[]', 'deny_all_egress', '[]', 1, 1, 1)";

/// A runner with no assignment, which only slot 600's shape allowed.
const UNASSIGNED: &str = "\
INSERT INTO fleet.runners (id, host_id, token_hash, sandbox_tier, admin_state, labels,
  last_seen_at, created_at, updated_at)
VALUES ($1::uuid, $1, $1, 'dev_none', 'active', '[]', 1, 1, 1)";

/// Whether each assignment column still admits NULL.
const NULLABILITY: &str = "\
SELECT string_agg(column_name || '=' || is_nullable, ',' ORDER BY column_name)
FROM information_schema.columns WHERE table_schema = 'fleet' AND table_name = 'runners'
  AND column_name IN ('network_policy', 'registry_allowlist', 'extra_binds')";

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_runner_assignment_is_required_and_assigned_rows_survive() {
    let database = TestDatabase::create().await;
    let db = database.open(DbRole::Migrator, &[]).await;
    migrate_through_before(&db).await;
    let kept = mint_id();
    write(&db, ASSIGNED, &kept)
        .await
        .expect("an assigned runner writes");
    assert_eq!(
        nullability(&db).await,
        "extra_binds=YES,network_policy=YES,registry_allowlist=YES"
    );

    let carried = Migrator::new().run(&db).await.expect("955 applies");
    assert_eq!(carried.applied, [RUNNER_POLICY]);
    assert_eq!(
        nullability(&db).await,
        "extra_binds=YES,network_policy=NO,registry_allowlist=NO",
        "the assignment is required; extra binds keep NULL as their empty value"
    );
    let refused = write(&db, UNASSIGNED, &mint_id())
        .await
        .expect_err("a runner without an assignment is refused");
    assert_eq!(sqlstate(&refused), Some(NOT_NULL_VIOLATION.to_owned()));
    let survivors: i64 = sqlx::query("SELECT count(*) FROM fleet.runners WHERE id = $1::uuid")
        .bind(&kept)
        .fetch_one(&mut *db.acquire().await.unwrap())
        .await
        .unwrap()
        .try_get(0)
        .unwrap();
    assert_eq!(survivors, 1, "the assigned runner is kept");

    db.close().await;
    database.cleanup().await;
}

/// A row from before the rule fails the slot instead of being guessed at.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_an_unassigned_runner_row_fails_the_slot_and_changes_nothing() {
    let database = TestDatabase::create().await;
    let db = database.open(DbRole::Migrator, &[]).await;
    migrate_through_before(&db).await;
    write(&db, UNASSIGNED, &mint_id())
        .await
        .expect("slot 600's shape admits an unassigned runner");

    Migrator::new()
        .run(&db)
        .await
        .expect_err("an unassigned row fails the slot");
    assert_eq!(
        nullability(&db).await,
        "extra_binds=YES,network_policy=YES,registry_allowlist=YES",
        "the failed slot left the columns as they were"
    );

    db.close().await;
    database.cleanup().await;
}

async fn migrate_through_before(db: &Db) {
    Migrator::new()
        .with_migrations(through(BEFORE))
        .run(db)
        .await
        .expect("the list through slot 954 applies");
}

async fn write(db: &Db, statement: &'static str, id: &str) -> Result<(), sqlx::Error> {
    sqlx::query(statement)
        .bind(id)
        .execute(&mut *db.acquire().await.unwrap())
        .await
        .map(drop)
}

async fn nullability(db: &Db) -> String {
    sqlx::query(NULLABILITY)
        .fetch_one(&mut *db.acquire().await.unwrap())
        .await
        .unwrap()
        .try_get(0)
        .unwrap()
}

fn sqlstate(error: &sqlx::Error) -> Option<String> {
    error
        .as_database_error()
        .and_then(sqlx::error::DatabaseError::code)
        .map(std::borrow::Cow::into_owned)
}
