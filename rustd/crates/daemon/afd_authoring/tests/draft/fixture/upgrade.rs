//! A release migration runs while the previous daemon may still accept work.

use super::{Fixture, TestDatabase, mint};
use afd_core::id::Uuid7;
use afd_db::{Migrator, config::DbRole, migration::MIGRATIONS};

const DEADLINE_SLOT: i64 = 2130;
const PENDING: &str = "pending";
// Pin the writer that predates the deadline migration, including the old message
// column list. Today's admission code requires later schema slots.
const LEGACY_DRAFT: &str = "\
INSERT INTO authoring.drafts
    (id,workspace_id,tenant_id,user_id,title,status,source_type,tenant_model_id,
     last_message_sequence,cancelled_through_sequence,resource_version,created_at,updated_at)
VALUES ($1::uuid,$2::uuid,$3::uuid,$4::uuid,'Dispatch upgrade','editing','blank',$5::uuid,
        1,0,1,now(),now())";
const LEGACY_MESSAGE: &str = "\
INSERT INTO authoring.messages
    (id,draft_id,sequence,operation_id,status,user_text,created_at,updated_at)
VALUES ($1::uuid,$1::uuid,1,$1::uuid,'running','Investigate',now(),now())";
const LEGACY_ATTEMPT: &str = "\
INSERT INTO authoring.attempts
    (id,lease_id,workspace_id,tenant_id,user_id,draft_id,message_sequence,fencing_token,
     model_token_hash,model_snapshot,status,lease_expires_at,expires_at,reserved_nanos,
     usage_unknown,call_count,output_token_count,reported_runtime_ms,last_activity_sequence,created_at,updated_at)
VALUES ($1::uuid,$1::uuid,$2::uuid,$3::uuid,$4::uuid,$1::uuid,1,1,$1,$5::jsonb,'active',
        now()+interval '1 minute',now()+interval '1 minute',0,false,0,0,0,0,now(),now())";
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
    let attempt = legacy_attempt(&f).await;
    sqlx::query("UPDATE authoring.attempts SET expires_at=clock_timestamp()+interval '5 seconds' WHERE id=$1::uuid")
        .bind(attempt.as_str())
        .execute(&mut *f.connection().await)
        .await
        .expect("an attempt shorter than a legacy physical call");
    legacy_insert(&f, &attempt, 1, 90.0).await;
    legacy_insert(&f, &attempt, 2, 0.0).await;

    let upgraded = Migrator::new()
        .run(&database)
        .await
        .expect("apply the new deadline slot to populated dispatches");
    let expected: Vec<_> = MIGRATIONS
        .iter()
        .skip(preceding)
        .map(afd_db::migration::Migration::version)
        .collect();
    assert_eq!(upgraded.applied, expected);
    assert_backfill(&f, &attempt).await;

    // The old process still writes its old column list after the release
    // command finishes and before its replacement starts serving.
    let old_writer = legacy_insert(&f, &attempt, 3, 0.0).await;
    assert_old_writer(&f, &old_writer).await;

    database.close().await;
    f.cleanup().await;
}

async fn legacy_attempt(f: &Fixture) -> Uuid7 {
    let id = mint();
    let mut connection = f.connection().await;
    let model = afd_credential::provider::registered_model(
        &mut connection,
        &f.tenant,
        &f.model.to_string(),
    )
    .await
    .expect("registered historical model")
    .expect("fixture model exists");
    sqlx::query(LEGACY_DRAFT)
        .bind(id.as_str())
        .bind(f.workspace.as_str())
        .bind(f.tenant.as_str())
        .bind(f.owner.as_str())
        .bind(f.model.as_str())
        .execute(&mut *connection)
        .await
        .expect("old writer creates its draft");
    sqlx::query(LEGACY_MESSAGE)
        .bind(id.as_str())
        .execute(&mut *connection)
        .await
        .expect("old writer admits a message without future save columns");
    sqlx::query(LEGACY_ATTEMPT)
        .bind(id.as_str())
        .bind(f.workspace.as_str())
        .bind(f.tenant.as_str())
        .bind(f.owner.as_str())
        .bind(serde_json::to_string(&model).expect("historical model metadata"))
        .execute(&mut *connection)
        .await
        .expect("old writer starts its physical call");
    id
}

async fn assert_backfill(f: &Fixture, attempt: &Uuid7) {
    let backfilled: (i64, bool, bool) = sqlx::query_as(
        "\
SELECT count(*),
       bool_and(d.dispatch_expires_at=LEAST(a.expires_at,d.created_at+interval '30 seconds')),
       bool_and(d.failure_cause IS NULL)
  FROM authoring.dispatches d JOIN authoring.attempts a ON a.id=d.attempt_id
 WHERE a.id=$1::uuid",
    )
    .bind(attempt.as_str())
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

async fn legacy_insert(f: &Fixture, attempt: &Uuid7, sequence: i32, age: f64) -> Uuid7 {
    let id = mint();
    sqlx::query(LEGACY_INSERT)
        .bind(id.as_str())
        .bind(attempt.as_str())
        .bind(sequence)
        .bind(PENDING)
        .bind(age)
        .execute(&mut *f.connection().await)
        .await
        .expect("the previous daemon's insert works before and after migration");
    id
}
