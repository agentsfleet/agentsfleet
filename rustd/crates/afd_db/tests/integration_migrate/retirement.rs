//! Slots 951–954 against a populated database: what they delete, and what stays.
//!
//! Each proof migrates a scratch database through slot 950, writes the rows the
//! later slots act on, then carries it forward with the production migrator.

use super::cutover_rows::{through, through_cutover};
use super::*;
use afd_db::Db;
use afd_db::test_util::mint_id;

/// The slots this file proves, in application order.
const RETIREMENT: [i32; 4] = [951, 952, 953, LAST_RETIRED];

/// The last slot this file proves; later slots carry their own proofs.
const LAST_RETIRED: i32 = 954;

/// One fleet with a session and two obligations, one of which names nowhere.
///
/// One statement, for the reason `cutover_rows::SEED` gives.
const SEED: &str = "\
WITH tenant AS (
  INSERT INTO core.tenants (id, name, created_at, updated_at)
  VALUES ($1::uuid, 'retirement', 1, 2)
), workspace AS (
  INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at)
  VALUES ($2::uuid, $1::uuid, 'retirement', 'user_retirement', 1)
), fleet AS (
  INSERT INTO core.fleets (id, workspace_id, tenant_id, name, config_json, status,
    created_at, updated_at)
  VALUES ($3::uuid, $2::uuid, $1::uuid, 'retirement', '{}', 'active', 1, 2)
), session AS (
  INSERT INTO core.fleet_sessions (fleet_id, checkpoint_at, execution_id,
    execution_started_at, created_at, updated_at)
  VALUES ($3::uuid, 1, 'stale-handle', 1, 1, 2)
), nowhere AS (
  INSERT INTO core.fleet_obligations (id, fleet_id, workspace_id, provider, destination,
    event_id, answer, attempt_count, created_at, updated_at)
  VALUES ($4::uuid, $3::uuid, $2::uuid, 'anthropic', NULL, '1-0', 'lost', 0, 1, 1)
)
INSERT INTO core.fleet_obligations (id, fleet_id, workspace_id, provider, destination,
  event_id, answer, attempt_count, created_at, updated_at)
VALUES ($5::uuid, $3::uuid, $2::uuid, 'slack', '{\"channel_id\":\"C1\"}', '1-1', 'kept', 0,
  1, 1)";

/// What the four slots leave: obligations, nullability, session columns,
/// repair tables, repair functions, the session row, and slot 835's fleets
/// index, which is not the repair storage's.
const READINGS: &str = "\
SELECT
  (SELECT string_agg(answer, ',' ORDER BY answer) FROM core.fleet_obligations
    WHERE fleet_id = $1::uuid),
  (SELECT is_nullable FROM information_schema.columns WHERE table_schema = 'core'
    AND table_name = 'fleet_obligations' AND column_name = 'destination'),
  (SELECT count(*) FROM information_schema.columns WHERE table_schema = 'core'
    AND table_name = 'fleet_sessions'
    AND column_name IN ('execution_id', 'execution_started_at')),
  (SELECT count(*) FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace
    WHERE n.nspname = 'core' AND c.relkind = 'r' AND c.relname LIKE 'repair\\_%'),
  (SELECT count(*) FROM pg_proc p JOIN pg_namespace n ON n.oid = p.pronamespace
    WHERE n.nspname = 'core' AND p.proname LIKE 'repair\\_%'),
  (SELECT count(*) FROM core.fleet_sessions WHERE fleet_id = $1::uuid),
  (SELECT count(*) FROM pg_indexes WHERE schemaname = 'core'
    AND indexname = 'idx_fleets_workspace_status_id'),
  (SELECT count(*) FROM information_schema.columns WHERE
    (table_schema, table_name, column_name) IN (
      ('billing', 'authoring_settlements', 'answer_digest'),
      ('fleet', 'authoring_dispatches', 'provider_continuation'),
      ('core', 'api_keys', 'last_used_at'),
      ('core', 'fleet_events', 'checkpoint_id'),
      ('fleet', 'runners', 'tenant_id'))),
  (SELECT count(*) FROM pg_indexes WHERE schemaname = 'fleet'
    AND indexname = 'idx_authoring_dispatches_private_expiry'
    AND indexdef NOT LIKE '%provider_continuation%')";

type Readings = (
    Option<String>,
    Option<String>,
    i64,
    i64,
    i64,
    i64,
    i64,
    i64,
    i64,
);

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_retirement_slots_drop_dead_storage_and_keep_live_rows() {
    let database = TestDatabase::create().await;
    let db = database.open(DbRole::Migrator, &[]).await;
    Migrator::new()
        .with_migrations(through_cutover())
        .run(&db)
        .await
        .expect("the list through slot 950 applies");
    let fleet = seed(&db).await;
    assert_eq!(
        readings(&db, &fleet).await,
        (
            Some("kept,lost".to_owned()),
            Some("YES".to_owned()),
            2,
            4,
            4,
            1,
            1,
            5,
            0
        ),
        "950 holds both obligations, the session and dead columns, and the repair storage"
    );

    let carried = Migrator::new()
        .with_migrations(through(LAST_RETIRED))
        .run(&db)
        .await
        .expect("951–954 apply");
    assert_eq!(carried.applied, RETIREMENT);
    let after = (
        Some("kept".to_owned()),
        Some("NO".to_owned()),
        0,
        0,
        0,
        1,
        1,
        0,
        1,
    );
    assert_eq!(readings(&db, &fleet).await, after);

    let repeated = Migrator::new()
        .with_migrations(through(LAST_RETIRED))
        .run(&db)
        .await
        .expect("a rerun is idempotent");
    assert_eq!(repeated.applied, [] as [i32; 0]);
    assert_eq!(readings(&db, &fleet).await, after);

    db.close().await;
    database.cleanup().await;
}

/// Writes the seed and answers the fleet it names.
async fn seed(db: &Db) -> String {
    let fleet = mint_id();
    sqlx::query(SEED)
        .bind(mint_id())
        .bind(mint_id())
        .bind(&fleet)
        .bind(mint_id())
        .bind(mint_id())
        .execute(&mut *db.acquire().await.unwrap())
        .await
        .expect("the seed writes");
    fleet
}

async fn readings(db: &Db, fleet: &str) -> Readings {
    sqlx::query_as(READINGS)
        .bind(fleet)
        .fetch_one(&mut *db.acquire().await.unwrap())
        .await
        .unwrap()
}
