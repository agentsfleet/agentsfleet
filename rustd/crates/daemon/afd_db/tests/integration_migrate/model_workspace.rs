//! The forward model migration preserves identities and pins legacy sources once.

use std::sync::LazyLock;

use af_core::id::Uuid7;
use afd_db::test_util::{TenantSeed, WorkspaceSeed, mint_id};
use jiff::Timestamp;

use super::*;

const WORKSPACE_SLOT: i64 = 20_261_111_230_000;
static BEFORE: LazyLock<Vec<Migration>> = LazyLock::new(|| {
    MIGRATIONS
        .iter()
        .copied()
        .filter(|migration| migration.version() < WORKSPACE_SLOT)
        .collect()
});

const INSERT_LEGACY: &str = "INSERT INTO model.tenant_models
    (id, tenant_id, model_name, secret_ref, created_at, updated_at)
    VALUES ($1, $2, $3, $4, now(), now())";

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn model_workspace_migration_retains_entries_and_unavailable_history() {
    let database = TestDatabase::create().await;
    let db = database.open(DbRole::Migrator, &[]).await;
    Migrator::new()
        .with_migrations(&BEFORE)
        .run(&db)
        .await
        .expect("the previous schema applies");
    let tenant = mint_id();
    let orphan = mint_id();
    let main = mint_id();
    let staging = mint_id();
    let entry = mint_id();
    let missing = mint_id();
    let mut connection = db.acquire().await.expect("migration fixture connection");
    for owner in [&tenant, &orphan] {
        TenantSeed::new(owner)
            .insert(&mut *connection)
            .await
            .expect("the account seeds");
    }
    for (workspace, at) in [(&main, 1_760_000_000), (&staging, 1_760_000_001)] {
        WorkspaceSeed::new(workspace, &tenant)
            .at(Timestamp::constant(at, 0))
            .insert(&mut *connection)
            .await
            .expect("ordered workspaces seed");
    }
    for (id, owner) in [(&entry, &tenant), (&missing, &orphan)] {
        sqlx::query(INSERT_LEGACY)
            .bind(id)
            .bind(owner)
            .bind("model")
            .bind("key")
            .execute(&mut *connection)
            .await
            .expect("an old model entry seeds");
    }
    sqlx::query(
        "INSERT INTO model.tenant_selections (tenant_id, tenant_model_id, created_at, updated_at)
        VALUES ($1, $2, now(), now())",
    )
    .bind(&tenant)
    .bind(&entry)
    .execute(&mut *connection)
    .await
    .expect("the old selection seeds");
    let created: jiff_sqlx::Timestamp =
        sqlx::query_scalar("SELECT created_at FROM model.tenant_models WHERE id=$1")
            .bind(&entry)
            .fetch_one(&mut *connection)
            .await
            .expect("the original timestamp reads");
    drop(connection);

    let applied = Migrator::new()
        .run(&db)
        .await
        .expect("the forward migration applies");
    assert_eq!(applied.applied, vec![WORKSPACE_SLOT]);
    let mut connection = db.acquire().await.expect("migrated connection");
    assert_migrated_history(&mut connection, &entry, &main, &missing, &created).await;

    // The same model/key name is a different reference in Staging.
    sqlx::query("INSERT INTO model.tenant_models (id, tenant_id, workspace_id, model_name, secret_ref, created_at, updated_at)
        VALUES ($1,$2,$3,$4,$5,now(),now())")
        .bind(mint_id()).bind(&tenant).bind(&staging).bind("model").bind("key")
        .execute(&mut *connection).await.expect("a second explicit namespace does not collide");
    let foreign = sqlx::query("UPDATE model.tenant_models SET workspace_id=$2 WHERE id=$1")
        .bind(&missing)
        .bind(&staging)
        .execute(&mut *connection)
        .await
        .expect_err("the database refuses a workspace from another account");
    assert!(afd_db::constraint::violates_foreign_key(
        &foreign,
        "fk_tenant_models_workspace"
    ));

    assert_removed_workspace_history(&mut connection, &main, &staging, &entry, &tenant).await;
    drop(connection);
    db.close().await;
    database.cleanup().await;
}

async fn assert_migrated_history(
    connection: &mut sqlx::PgConnection,
    entry: &Uuid7,
    main: &Uuid7,
    missing: &Uuid7,
    created: &jiff_sqlx::Timestamp,
) {
    let source: Option<Uuid7> =
        sqlx::query_scalar("SELECT workspace_id FROM model.tenant_models WHERE id=$1")
            .bind(entry)
            .fetch_one(&mut *connection)
            .await
            .expect("the source reads");
    assert_eq!(
        source,
        Some(main.clone()),
        "the entry retains the old resolver's actual namespace"
    );
    let unavailable: Option<Uuid7> =
        sqlx::query_scalar("SELECT workspace_id FROM model.tenant_models WHERE id=$1")
            .bind(missing)
            .fetch_one(&mut *connection)
            .await
            .expect("unavailable history survives");
    assert_eq!(
        unavailable, None,
        "a workspace is never fabricated for old orphaned history"
    );
    let unchanged: jiff_sqlx::Timestamp =
        sqlx::query_scalar("SELECT created_at FROM model.tenant_models WHERE id=$1")
            .bind(entry)
            .fetch_one(&mut *connection)
            .await
            .expect("the timestamp remains");
    assert_eq!(created, &unchanged);
}

async fn assert_removed_workspace_history(
    connection: &mut sqlx::PgConnection,
    main: &Uuid7,
    staging: &Uuid7,
    entry: &Uuid7,
    tenant: &Uuid7,
) {
    sqlx::query("DELETE FROM identity.workspaces WHERE id=$1")
        .bind(main)
        .execute(&mut *connection)
        .await
        .expect("workspace removal retains model history");
    let retained: (Option<Uuid7>, Uuid7) = sqlx::query_as("SELECT m.workspace_id, s.tenant_model_id
        FROM model.tenant_models m JOIN model.tenant_selections s ON s.tenant_model_id=m.id WHERE m.id=$1")
        .bind(entry).fetch_one(&mut *connection).await.expect("the original entry and selection survive");
    assert_eq!(
        retained,
        (None, entry.clone()),
        "no replacement source or default is selected"
    );
    sqlx::query("DELETE FROM identity.workspaces WHERE id=$1")
        .bind(staging)
        .execute(&mut *connection)
        .await
        .expect("losing a second same-named source preserves both histories");
    let historical: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM model.tenant_models WHERE tenant_id=$1 AND workspace_id IS NULL",
    )
    .bind(tenant)
    .fetch_one(&mut *connection)
    .await
    .expect("both historical entries remain");
    assert_eq!(historical, 2);
}
