//! Fresh bootstrap exposes the queue stamps and index to the actual API role.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "a failed datastore precondition fails the test"
)]

use afd_db::{Migrator, config::DbRole, test_util::TestDatabase};

const QUEUE_MIGRATION: i64 = 2110;
const RECEIPT_DROP_MIGRATION: i64 = 2120;
const DISPATCH_DEADLINE_MIGRATION: i64 = 2130;

// M230 can land before M226: its higher slot must not hide the queue upgrade.
static DISPATCH_ONLY: std::sync::LazyLock<Vec<afd_db::migration::Migration>> =
    std::sync::LazyLock::new(|| {
        afd_db::migration::MIGRATIONS
            .iter()
            .copied()
            .filter(|migration| {
                ![QUEUE_MIGRATION, RECEIPT_DROP_MIGRATION].contains(&migration.version())
            })
            .collect()
    });

/// Type, nullability, default and timestamp precision of each added column.
const COLUMNS: &str = "SELECT column_name::text, data_type::text, is_nullable::text, \
    column_default::text, datetime_precision FROM information_schema.columns \
    WHERE table_schema = 'fleet' AND table_name = 'admissions' \
      AND column_name = ANY($1) ORDER BY column_name";
/// The actual index expression, including its column order and partial predicate.
const INDEX: &str = "SELECT indexdef FROM pg_indexes \
    WHERE schemaname = 'fleet' AND indexname = 'idx_admissions_unsettled'";
/// A zero-row update still checks privileges for all three new columns.
const UPDATE: &str = "UPDATE fleet.admissions \
    SET lease_fence = 1, parked_at = now(), settled_at = now() WHERE false";

#[derive(sqlx::FromRow)]
struct SchemaField {
    column_name: String,
    data_type: String,
    is_nullable: String,
    column_default: Option<String>,
    datetime_precision: Option<i32>,
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_admissions_queue_migration_bootstraps() {
    let lane = TestDatabase::create().await;
    let migrator = lane.open(DbRole::Migrator, &[]).await;
    Migrator::new()
        .run(&migrator)
        .await
        .expect("fresh bootstrap");
    let api = lane.open(DbRole::Api, &[]).await;
    let mut connection = api.acquire().await.expect("runtime connection");
    let columns: Vec<SchemaField> = sqlx::query_as(COLUMNS)
        .bind(&["lease_fence", "parked_at", "settled_at"][..])
        .fetch_all(&mut *connection)
        .await
        .expect("runtime sees queue columns");
    let expected = [
        ("lease_fence", "bigint", None),
        ("parked_at", "timestamp with time zone", Some(3)),
        ("settled_at", "timestamp with time zone", Some(3)),
    ];
    assert_eq!(columns.len(), expected.len());
    for (found, (name, kind, precision)) in columns.iter().zip(expected) {
        assert_eq!(
            (
                found.column_name.as_str(),
                found.data_type.as_str(),
                found.is_nullable.as_str()
            ),
            (name, kind, "YES")
        );
        assert!(found.column_default.is_none(), "new stamps start unset");
        assert_eq!(found.datetime_precision, precision);
    }
    let index: String = sqlx::query_scalar(INDEX)
        .fetch_one(&mut *connection)
        .await
        .expect("partial queue index");
    assert!(
        index.contains("(fleet_id, created_at, sequence) WHERE (settled_at IS NULL)"),
        "{index}"
    );
    sqlx::query(UPDATE)
        .execute(&mut *connection)
        .await
        .expect("runtime may move queue state");
    drop(connection);
    api.close().await;
    migrator.close().await;
    lane.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_receipt_drop_migration_applies_populated() {
    let lane = TestDatabase::create().await;
    let db = lane.open(DbRole::Migrator, &[]).await;
    let migrations = afd_db::migration::MIGRATIONS;
    let removal = migrations
        .iter()
        .position(|migration| migration.version() == RECEIPT_DROP_MIGRATION)
        .expect("receipt removal migration");
    let before = migrations
        .get(..removal)
        .expect("migrations before removal");
    Migrator::new()
        .with_migrations(before)
        .run(&db)
        .await
        .expect("schema before removal");
    let (fleet, admission) = populate_before_drop(&db).await;
    let outcome = Migrator::new().run(&db).await.expect("populated upgrade");
    assert_eq!(
        outcome.applied,
        [RECEIPT_DROP_MIGRATION, DISPATCH_DEADLINE_MIGRATION]
    );
    let mut connection = db.acquire().await.expect("database");
    let old_columns: i64 = sqlx::query_scalar("SELECT count(*) FROM information_schema.columns WHERE (table_schema='fleet' AND table_name='admissions' AND column_name=ANY($1)) OR (table_schema='runner' AND table_name='leases' AND column_name='receipt')")
        .bind(&["receipt", "replay_count", "replay_claim_expires_at"][..])
        .fetch_one(&mut *connection).await.expect("removed columns");
    assert_eq!(old_columns, 0);
    let stored: String = sqlx::query_scalar(
        "SELECT id::text FROM fleet.admissions WHERE fleet_id=$1::uuid AND settled_at IS NULL",
    )
    .bind(&fleet)
    .fetch_one(&mut *connection)
    .await
    .expect("admission preserved");
    assert_eq!(stored, admission);
    sqlx::query("UPDATE fleet.admissions SET lease_fence=1, delivered_at=clock_timestamp(), settled_at=clock_timestamp() WHERE id=$1::uuid")
        .bind(&admission).execute(&mut *connection).await.expect("preserved row still moves");
    drop(connection);
    db.close().await;
    lane.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_queue_upgrade_after_dispatch_deadline_preserves_admission() {
    let lane = TestDatabase::create().await;
    let db = lane.open(DbRole::Migrator, &[]).await;
    let installed = Migrator::new()
        .with_migrations(DISPATCH_ONLY.as_slice())
        .run(&db)
        .await
        .expect("M230 schema without the admissions queue upgrade");
    assert!(installed.applied.contains(&DISPATCH_DEADLINE_MIGRATION));
    let (_, admission) = populate_before_drop(&db).await;
    let outcome = Migrator::new().run(&db).await.expect("M226 after M230");
    assert_eq!(outcome.applied, [QUEUE_MIGRATION, RECEIPT_DROP_MIGRATION]);
    assert!(outcome.skipped.contains(&DISPATCH_DEADLINE_MIGRATION));
    let mut connection = db.acquire().await.expect("database");
    let fence: i64 = sqlx::query_scalar(
        "UPDATE fleet.admissions SET lease_fence=1, settled_at=clock_timestamp() \
         WHERE id=$1::uuid RETURNING lease_fence",
    )
    .bind(&admission)
    .fetch_one(&mut *connection)
    .await
    .expect("preserved admission accepts the queue transition");
    assert_eq!(fence, 1);
    let dispatch_columns: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM information_schema.columns \
         WHERE table_schema='authoring' AND table_name='dispatches' \
         AND column_name IN ('dispatch_expires_at', 'failure_cause')",
    )
    .fetch_one(&mut *connection)
    .await
    .expect("M230 dispatch columns survive");
    assert_eq!(dispatch_columns, 2);
    drop(connection);
    let repeated = Migrator::new().run(&db).await.expect("repeat upgrade");
    assert_eq!(repeated.applied, [] as [i64; 0]);
    assert_eq!(repeated.skipped.len(), afd_db::migration::MIGRATIONS.len());
    db.close().await;
    lane.cleanup().await;
}

async fn populate_before_drop(db: &afd_db::Db) -> (String, String) {
    use afd_db::test_util::{FleetSeed, TenantSeed, WorkspaceSeed, mint_id};
    let (tenant, workspace, fleet, admission) = (mint_id(), mint_id(), mint_id(), mint_id());
    let mut connection = db.acquire().await.expect("database");
    TenantSeed::new(&tenant)
        .insert(&mut *connection)
        .await
        .expect("tenant");
    WorkspaceSeed::new(&workspace, &tenant)
        .insert(&mut *connection)
        .await
        .expect("workspace");
    FleetSeed::new(&fleet, &workspace, &tenant, "stopped")
        .native_setup()
        .insert(&mut *connection)
        .await
        .expect("fleet");
    sqlx::query("INSERT INTO fleet.admissions (id, fleet_id, workspace_id, producer, producer_key, payload_digest, actor, event_type, request_json, event_created_at, receipt, replay_count, replay_claim_expires_at, created_at, updated_at) VALUES ($1::uuid,$2::uuid,$3::uuid,'steer',$1,'fixture','fixture','chat','{}',now(),'1790000000000-0',2,clock_timestamp()+interval '1 hour',now(),now())")
        .bind(&admission).bind(&fleet).bind(&workspace)
        .execute(&mut *connection).await.expect("old populated admission");
    (fleet, admission)
}
