//! Dimension 3.1: slot 926 gives every existing entry its fleet's workspace and
//! leaves it fleet-visible, slot 927 grants no fleet anything, and running
//! either again changes nothing.
//!
//! An UPGRADE, not a fresh install: the database is migrated to the slot
//! before 926, populated, and only then carried forward — a fresh database has
//! no rows for a backfill to get wrong.
#![expect(
    clippy::expect_used,
    reason = "integration test: an unmet precondition should fail the test loudly"
)]

use afd_db::Migrator;
use afd_db::config::DbRole;
use afd_db::migration::MIGRATIONS;
use afd_db::test_util::{TestDatabase, mint_id};
use sqlx::Row as _;

/// The slot that adds the workspace scope to an entry.
const WORKSPACE_SCOPE: i32 = 926;
/// The slot that adds the two grants to a fleet.
const FLEET_ACCESS: i32 = 927;

/// Every entry's workspace and visibility, with its fleet's workspace beside it.
const SELECT_ENTRIES: &str = "\
SELECT entry.workspace_id::text, fleet.workspace_id::text, entry.workspace_visible
FROM memory.memory_entries AS entry
JOIN core.fleets AS fleet ON fleet.id = entry.fleet_id
ORDER BY entry.key";

/// Every fleet's two grants.
const SELECT_GRANTS: &str =
    "SELECT memory_reads_workspace, memory_publishes_workspace FROM core.fleets";

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_memory_migration_backfills_workspace() {
    let database = TestDatabase::create().await;
    let db = database.open(DbRole::Migrator, &[]).await;
    let boundary = MIGRATIONS
        .iter()
        .position(|migration| migration.version() == WORKSPACE_SCOPE)
        .expect("slot 926 is registered");
    let before = MIGRATIONS
        .get(..boundary)
        .expect("the slot's index is in range");
    Migrator::new()
        .with_migrations(before)
        .run(&db)
        .await
        .expect("the list before slot 926 applies");
    let mut connection = db.acquire().await.expect("a migrator connection");
    seed(&mut connection).await;

    let upgrade = Migrator::new()
        .run(&db)
        .await
        .expect("926 and 927 apply to live rows");
    assert!(upgrade.applied.contains(&WORKSPACE_SCOPE));
    assert!(upgrade.applied.contains(&FLEET_ACCESS));
    let backfilled = entries(&mut connection).await;
    assert_eq!(backfilled.len(), 2, "the upgrade kept every entry");
    for (entry_workspace, fleet_workspace, visible) in &backfilled {
        assert_eq!(
            entry_workspace, fleet_workspace,
            "each entry carries its fleet's workspace"
        );
        assert!(!visible, "and no existing entry becomes shared");
    }
    assert_eq!(
        grants(&mut connection).await,
        vec![(false, false)],
        "no fleet is granted"
    );

    for slot in [WORKSPACE_SCOPE, FLEET_ACCESS] {
        let migration = MIGRATIONS
            .iter()
            .find(|migration| migration.version() == slot)
            .expect("the slot is registered");
        sqlx::raw_sql(migration.sql())
            .execute(&mut *connection)
            .await
            .expect("a rerun of the slot applies cleanly");
    }
    assert_eq!(
        entries(&mut connection).await,
        backfilled,
        "a rerun changes nothing"
    );
    assert_eq!(grants(&mut connection).await, vec![(false, false)]);
    drop(connection);
    drop(db);
    database.cleanup().await;
}

/// One tenant, workspace and fleet, holding two entries written before 926.
async fn seed(connection: &mut sqlx::PgConnection) {
    let (tenant, workspace, fleet) = (mint_id(), mint_id(), mint_id());
    sqlx::query(
        "WITH tenant AS ( \
           INSERT INTO core.tenants (id, name, created_at, updated_at) \
           VALUES ($1::uuid, 'Upgrade', 1, 1) \
         ), workspace AS ( \
           INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at) \
           VALUES ($2::uuid, $1::uuid, 'upgrade', 'user_upgrade', 1) \
         ) \
         INSERT INTO core.fleets \
           (id, workspace_id, tenant_id, name, source_markdown, config_json, status, \
            created_at, updated_at) \
         VALUES ($3::uuid, $2::uuid, $1::uuid, 'upgrade', '# fixture', '{}', 'active', 1, 1)",
    )
    .bind(&tenant)
    .bind(&workspace)
    .bind(&fleet)
    .execute(&mut *connection)
    .await
    .expect("the fleet seeds before the upgrade");
    for key in ["deploy_target", "owner"] {
        sqlx::query(
            "INSERT INTO memory.memory_entries \
               (id, key, content, category, fleet_id, created_at, updated_at) \
             VALUES ($1::uuid, $2, 'a fact', 'core', $3::uuid, 1, 1)",
        )
        .bind(mint_id())
        .bind(key)
        .bind(&fleet)
        .execute(&mut *connection)
        .await
        .expect("an entry written before the upgrade");
    }
}

async fn entries(connection: &mut sqlx::PgConnection) -> Vec<(String, String, bool)> {
    sqlx::query(SELECT_ENTRIES)
        .fetch_all(&mut *connection)
        .await
        .expect("reading the upgraded entries")
        .iter()
        .map(|row| {
            (
                row.try_get(0).expect("the entry's workspace"),
                row.try_get(1).expect("the fleet's workspace"),
                row.try_get(2).expect("the visibility"),
            )
        })
        .collect()
}

async fn grants(connection: &mut sqlx::PgConnection) -> Vec<(bool, bool)> {
    sqlx::query(SELECT_GRANTS)
        .fetch_all(&mut *connection)
        .await
        .expect("reading the grants")
        .iter()
        .map(|row| {
            (
                row.try_get(0).expect("the read grant"),
                row.try_get(1).expect("the publish grant"),
            )
        })
        .collect()
}
