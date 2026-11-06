//! Slot 2700 counts the unsettled admissions a live database already holds.

use afd_db::test_util::{FleetSeed, TenantSeed, WorkspaceSeed, mint_id};

use super::*;

/// The slot whose backfill is under test.
const BACKLOG_SLOT: &str = "2700_fleet_admission_backlogs.sql";

/// One admission on `fleet`, settled when `settled` says so.
const INSERT_ADMISSION: &str = "\
INSERT INTO fleet.admissions (id, fleet_id, workspace_id, producer, producer_key, payload_digest, \
 actor, event_type, request_json, event_created_at, created_at, updated_at, settled_at) \
VALUES ($1::uuid, $2::uuid, $3::uuid, 'steer', $1, 'fixture', 'fixture', 'chat', '{}', now(), \
 now(), now(), CASE WHEN $4 THEN clock_timestamp() END)";

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn backfill_counts_the_unsettled_rows_it_finds() {
    let lane = TestDatabase::create().await;
    let database = lane.open(DbRole::Migrator, &[]).await;
    let boundary = MIGRATIONS
        .iter()
        .position(|migration| migration.name() == BACKLOG_SLOT)
        .unwrap();
    Migrator::new()
        .with_migrations(MIGRATIONS.get(..boundary).unwrap())
        .run(&database)
        .await
        .unwrap();
    let (tenant, workspace, fleet) = (mint_id(), mint_id(), mint_id());
    let mut connection = database.acquire().await.unwrap();
    TenantSeed::new(&tenant)
        .insert(&mut *connection)
        .await
        .unwrap();
    WorkspaceSeed::new(&workspace, &tenant)
        .insert(&mut *connection)
        .await
        .unwrap();
    FleetSeed::new(&fleet, &workspace, &tenant, "stopped")
        .native_setup()
        .insert(&mut *connection)
        .await
        .unwrap();
    for settled in [false, false, false, true] {
        sqlx::query(INSERT_ADMISSION)
            .bind(mint_id())
            .bind(&fleet)
            .bind(&workspace)
            .bind(settled)
            .execute(&mut *connection)
            .await
            .unwrap();
    }
    drop(connection);
    Migrator::new().run(&database).await.unwrap();
    let mut connection = database.acquire().await.unwrap();
    let count: i64 = sqlx::query_scalar(
        "SELECT unsettled_count FROM fleet.admission_backlogs WHERE fleet_id = $1::uuid",
    )
    .bind(&fleet)
    .fetch_one(&mut *connection)
    .await
    .unwrap();
    assert_eq!(count, 3);
    drop(connection);
    database.close().await;
    lane.cleanup().await;
}
