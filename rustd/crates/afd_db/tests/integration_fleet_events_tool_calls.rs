//! Slot 924's `tool_calls` column, and what applying it does to event rows a
//! deployed database already holds.
//!
//! The column is nullable with no default, and NULL means "not recorded". So
//! the claim worth proving is on an UPGRADE: rows written before the slot read
//! NULL afterwards, with every other column as it was. A fresh database has no
//! rows for the slot to touch, which is why this builds the database a deployed
//! instance is — migrated to the slot before, populated, then upgraded.
//!
//! Marked `#[ignore]`; `make test-integration-rustd` runs it.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_db::Migrator;
use afd_db::config::DbRole;
use afd_db::migration::MIGRATIONS;
use afd_db::test_util::TestDatabase;
use sqlx::Row as _;

/// The slot under test.
const TOOL_CALLS_SLOT: i32 = 924;

/// Identifiers the `uuidv7` CHECK on each parent table accepts.
const TENANT: &str = "01990000-0000-7000-8000-000000000924";
/// The workspace the seeded events belong to.
const WORKSPACE: &str = "01990000-0000-7000-8000-000000000925";
/// The fleet that ran them.
const FLEET: &str = "01990000-0000-7000-8000-000000000926";

/// The events written before the upgrade.
const EVENTS: [&str; 2] = ["1760000000000-0", "1760000000001-0"];

/// The answer each seeded event settled with — what the upgrade must keep.
const ANSWER: &str = "the run answered before the column existed";

/// An instant every timestamp column on the seeded rows carries.
const AT: i64 = 1_760_000_000_000;

/// Dimension 3.5. Migrating a populated database leaves existing rows NULL.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_tool_calls_column_upgrade_keeps_rows_null() {
    let database = TestDatabase::create().await;
    let db = database.open(DbRole::Migrator, &[]).await;

    let boundary = MIGRATIONS
        .iter()
        .position(|migration| migration.version() == TOOL_CALLS_SLOT)
        .expect("slot 924 is registered");
    let head = MIGRATIONS
        .get(..boundary)
        .expect("the slot's own index is within its own list");
    Migrator::new()
        .with_migrations(head)
        .run(&db)
        .await
        .expect("the list up to slot 924 applies to a fresh database");

    seed_settled_events(&db).await;

    let through = MIGRATIONS
        .get(..=boundary)
        .expect("the slot's own index is within its own list");
    let upgrade = Migrator::new()
        .with_migrations(through)
        .run(&db)
        .await
        .expect("slot 924 applies to a populated database");
    assert_eq!(
        upgrade.applied,
        vec![TOOL_CALLS_SLOT],
        "the upgrade must apply slot 924 and nothing else"
    );

    assert_column_shape(&db).await;

    let mut connection = db.acquire().await.expect("a pooled connection");
    let rows = sqlx::query(
        "SELECT event_id, response_text, tool_calls::text AS tool_calls
         FROM core.fleet_events WHERE fleet_id = $1::uuid ORDER BY event_id",
    )
    .bind(FLEET)
    .fetch_all(&mut *connection)
    .await
    .expect("reading the upgraded events");
    assert_eq!(rows.len(), EVENTS.len(), "the upgrade lost an event row");
    for (row, expected) in rows.iter().zip(EVENTS) {
        assert_eq!(
            row.try_get::<String, _>("event_id").expect("decodes"),
            expected
        );
        assert_eq!(
            row.try_get::<Option<String>, _>("response_text")
                .expect("decodes")
                .as_deref(),
            Some(ANSWER),
            "the upgrade must not touch the answer"
        );
        assert_eq!(
            row.try_get::<Option<String>, _>("tool_calls")
                .expect("decodes"),
            None,
            "a row from before the slot reads not-recorded"
        );
    }
    drop(connection);

    database.cleanup().await;
}

/// The column the slot asked for: JSONB, nullable, no default.
async fn assert_column_shape(db: &afd_db::Db) {
    let mut connection = db.acquire().await.expect("a pooled connection");
    let column = sqlx::query(
        "SELECT data_type, is_nullable, column_default
         FROM information_schema.columns
         WHERE table_schema = 'core' AND table_name = 'fleet_events'
           AND column_name = 'tool_calls'",
    )
    .fetch_one(&mut *connection)
    .await
    .expect("core.fleet_events.tool_calls exists after slot 924");
    assert_eq!(
        column.try_get::<String, _>("data_type").expect("decodes"),
        "jsonb"
    );
    assert_eq!(
        column.try_get::<String, _>("is_nullable").expect("decodes"),
        "YES"
    );
    assert_eq!(
        column
            .try_get::<Option<String>, _>("column_default")
            .expect("decodes"),
        None,
        "no default: a placeholder trace would read as calls nobody made"
    );
}

/// A tenant, a workspace, a fleet, and two events it settled.
async fn seed_settled_events(db: &afd_db::Db) {
    let mut connection = db.acquire().await.expect("a pooled connection");
    sqlx::query(
        "INSERT INTO core.tenants (id, name, created_at, updated_at)
         VALUES ($1::uuid, 'fixture', $2, $2)",
    )
    .bind(TENANT)
    .bind(AT)
    .execute(&mut *connection)
    .await
    .expect("seeding a tenant");
    sqlx::query(
        "INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at)
         VALUES ($1::uuid, $2::uuid, 'fixture', 'fixture', $3)",
    )
    .bind(WORKSPACE)
    .bind(TENANT)
    .bind(AT)
    .execute(&mut *connection)
    .await
    .expect("seeding a workspace");
    sqlx::query(
        "INSERT INTO core.fleets
           (id, workspace_id, tenant_id, name, source_markdown, config_json,
            status, created_at, updated_at)
         VALUES ($1::uuid, $2::uuid, $3::uuid, 'deploy-bot', '# fixture',
                 '{}'::jsonb, 'installed', $4, $4)",
    )
    .bind(FLEET)
    .bind(WORKSPACE)
    .bind(TENANT)
    .bind(AT)
    .execute(&mut *connection)
    .await
    .expect("seeding a fleet");
    for event in EVENTS {
        sqlx::query(
            "INSERT INTO core.fleet_events
               (fleet_id, workspace_id, event_id, actor, event_type, status,
                request_json, response_text, tokens, wall_ms, created_at, updated_at)
             VALUES ($1::uuid, $2::uuid, $3, 'steer:fixture', 'chat', 'processed',
                     '{\"message\":\"hello\"}', $4, 7, 12, $5, $5)",
        )
        .bind(FLEET)
        .bind(WORKSPACE)
        .bind(event)
        .bind(ANSWER)
        .bind(AT)
        .execute(&mut *connection)
        .await
        .expect("seeding a settled event");
    }
}
