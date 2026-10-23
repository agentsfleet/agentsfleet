//! A release migration runs while the previous daemon may still accept work.

use super::{Fixture, TestDatabase, mint};
use crate::draft::attempt::Attempt;
use afd_authoring::types::Source;
use afd_core::id::Uuid7;
use afd_db::{Migrator, config::DbRole, migration::MIGRATIONS};

const DEADLINE_SLOT: i64 = 2130;
const PENDING: &str = "pending";
const LEGACY_INSERT: &str = "\
INSERT INTO authoring.dispatches
    (id,attempt_id,sequence,request_digest,status,private_expires_at,created_at,updated_at)
VALUES ($1::uuid,$2::uuid,$3,$1,$4,clock_timestamp()+interval '1 day',
        clock_timestamp()-make_interval(secs=>$5),now())";

#[tokio::test]
#[ignore = "requires live Postgres: make test-integration-rustd"]
async fn dispatch_deadline_upgrade_preserves_old_rows_and_old_writers() {
    let lane = TestDatabase::create().await;
    let database = lane.open(DbRole::Migrator, &[]).await;
    let preceding = MIGRATIONS.partition_point(|migration| migration.version() < DEADLINE_SLOT);
    Migrator::new()
        .with_migrations(MIGRATIONS.get(..preceding).expect("known migration prefix"))
        .run(&database)
        .await
        .expect("schema served by the previous daemon");
    let f = Fixture::in_lane(lane).await;
    let attempt = Attempt::start(&f, f.access(), Source::Blank).await;
    sqlx::query("UPDATE authoring.attempts SET expires_at=clock_timestamp()+interval '5 seconds' WHERE id=$1::uuid")
        .bind(attempt.id.as_str())
        .execute(&mut *f.connection().await)
        .await
        .expect("an attempt shorter than a legacy physical call");
    legacy_insert(&f, &attempt, 1, 90.0).await;
    legacy_insert(&f, &attempt, 2, 0.0).await;

    let upgraded = Migrator::new()
        .run(&database)
        .await
        .expect("apply the new deadline slot to populated dispatches");
    assert_eq!(upgraded.applied, [DEADLINE_SLOT]);
    assert_backfill(&f, &attempt).await;

    // The old process still writes its old column list after the release
    // command finishes and before its replacement starts serving.
    let old_writer = legacy_insert(&f, &attempt, 3, 0.0).await;
    assert_old_writer(&f, &old_writer).await;

    database.close().await;
    f.cleanup().await;
}

async fn assert_backfill(f: &Fixture, attempt: &Attempt) {
    let backfilled: (i64, bool, bool) = sqlx::query_as(
        "\
SELECT count(*),
       bool_and(d.dispatch_expires_at=LEAST(a.expires_at,d.created_at+interval '30 seconds')),
       bool_and(d.failure_cause IS NULL)
  FROM authoring.dispatches d JOIN authoring.attempts a ON a.id=d.attempt_id
 WHERE a.id=$1::uuid",
    )
    .bind(attempt.id.as_str())
    .fetch_one(&mut *f.connection().await)
    .await
    .expect("backfilled deadlines retain the old physical and attempt bounds");
    assert_eq!(backfilled, (2, true, true));
}

async fn assert_old_writer(f: &Fixture, old_writer: &Uuid7) {
    let compatible: bool = sqlx::query_scalar("\
SELECT failure_cause IS NULL AND
       dispatch_expires_at BETWEEN created_at+interval '29 seconds' AND created_at+interval '31 seconds'
  FROM authoring.dispatches WHERE id=$1::uuid")
        .bind(old_writer.as_str())
        .fetch_one(&mut *f.connection().await)
        .await
        .expect("legacy writer receives its thirty-second physical bound");
    assert!(compatible, "old writers need a usable physical deadline");
}

async fn legacy_insert(f: &Fixture, attempt: &Attempt, sequence: i32, age: f64) -> Uuid7 {
    let id = mint();
    sqlx::query(LEGACY_INSERT)
        .bind(id.as_str())
        .bind(attempt.id.as_str())
        .bind(sequence)
        .bind(PENDING)
        .bind(age)
        .execute(&mut *f.connection().await)
        .await
        .expect("the previous daemon's insert works before and after migration");
    id
}
