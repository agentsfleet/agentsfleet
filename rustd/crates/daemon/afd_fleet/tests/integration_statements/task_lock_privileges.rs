//! Immutable parent locks run as the API role without granting mutation rights.

use af_core::id::Uuid7;
use afd_db::test_util::{FleetSeed, TenantSeed, WorkspaceSeed, mint_id};
use afd_fleet::task::{CHECKPOINT_WORKSPACE_LOCK_STATEMENT, TASK_SCOPE_LOCK_STATEMENT};
use sqlx::Connection as _;

use super::*;

struct Scope {
    workspace: Uuid7,
    other: Uuid7,
    unfinished: Uuid7,
    fleet: Uuid7,
}

async fn seed(db: &afd_db::Db) -> Scope {
    let tenant = mint_id();
    let scope = Scope {
        workspace: mint_id(),
        other: mint_id(),
        unfinished: mint_id(),
        fleet: mint_id(),
    };
    let mut connection = db.acquire().await.expect("fixture connection");
    TenantSeed::new(&tenant)
        .insert(&mut *connection)
        .await
        .expect("tenant");
    for workspace in [&scope.workspace, &scope.other, &scope.unfinished] {
        WorkspaceSeed::new(workspace, &tenant)
            .insert(&mut *connection)
            .await
            .expect("workspace");
    }
    FleetSeed::new(&scope.fleet, &scope.workspace, &tenant, "active")
        .insert(&mut *connection)
        .await
        .expect("fleet");
    scope
}

async fn api(connection: &mut PgConnection) {
    sqlx::query("SET LOCAL ROLE api_runtime")
        .execute(connection)
        .await
        .expect("real API privileges");
}

async fn cannot_update(connection: &mut PgConnection) {
    let mut savepoint = connection.begin().await.expect("privilege savepoint");
    let failure = sqlx::query("UPDATE identity.workspaces SET name = name WHERE false")
        .execute(&mut *savepoint)
        .await
        .expect_err("workspace UPDATE remains forbidden");
    assert_eq!(
        failure
            .as_database_error()
            .and_then(sqlx::error::DatabaseError::code)
            .as_deref(),
        Some("42501")
    );
    savepoint
        .rollback()
        .await
        .expect("restore transaction after denied UPDATE");
}

// Value: protects=API run and checkpoint transactions hold exact immutable parents;
// fails_when=runtime needs UPDATE privilege, loses its deletion fence, or locks unrelated workspaces;
// why_new=table grant probes did not exercise row locks; seam=exported production SQL;
// no_claim=full run issuance or remote object storage.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn api_parent_locks_preserve_immutability_and_exact_scope() {
    let (lane, db) = migrated().await;
    let scope = seed(&db).await;
    let mut connection = db.acquire().await.expect("lock owner");
    let mut held = connection.begin().await.expect("parent transaction");
    api(&mut held).await;
    let locked: Uuid7 = sqlx::query_scalar(TASK_SCOPE_LOCK_STATEMENT)
        .bind(&scope.fleet)
        .bind(&scope.workspace)
        .fetch_one(&mut *held)
        .await
        .expect("API scope lock");
    assert_eq!(locked, scope.fleet);
    let missing: Option<Uuid7> = sqlx::query_scalar(TASK_SCOPE_LOCK_STATEMENT)
        .bind(&scope.fleet)
        .bind(mint_id())
        .fetch_optional(&mut *held)
        .await
        .expect("absent scope");
    assert_eq!(missing, None);
    cannot_update(&mut held).await;
    let mut contender = db.acquire().await.expect("independent deleter");
    blocked_parent_and_unrelated_delete(&mut contender, &scope).await;
    held.commit().await.expect("release scope locks");

    let mut checkpoint = connection.begin().await.expect("checkpoint transaction");
    api(&mut checkpoint).await;
    let locked: Uuid7 = sqlx::query_scalar(CHECKPOINT_WORKSPACE_LOCK_STATEMENT)
        .bind(&scope.workspace)
        .fetch_one(&mut *checkpoint)
        .await
        .expect("API checkpoint parent lock");
    assert_eq!(locked, scope.workspace);
    cannot_update(&mut checkpoint).await;
    assert_parent_locked(&mut contender, &scope.workspace).await;
    checkpoint.commit().await.expect("release checkpoint lock");
    assert_eq!(
        sqlx::query("DELETE FROM identity.workspaces WHERE id=$1")
            .bind(&scope.workspace)
            .execute(&mut *contender)
            .await
            .expect("deletion proceeds after commit")
            .rows_affected(),
        1
    );
    drop(contender);
    drop(connection);
    db.close().await;
    lane.cleanup().await;
}

async fn blocked_parent_and_unrelated_delete(connection: &mut PgConnection, scope: &Scope) {
    assert_parent_locked(connection, &scope.workspace).await;
    assert_eq!(
        sqlx::query("DELETE FROM identity.workspaces WHERE id=$1")
            .bind(&scope.other)
            .execute(&mut *connection)
            .await
            .expect("unrelated deletion proceeds")
            .rows_affected(),
        1
    );
}

async fn assert_parent_locked(connection: &mut PgConnection, workspace: &Uuid7) {
    let mut deletion = connection.begin().await.expect("deletion attempt");
    let blocked = sqlx::query("SELECT id FROM identity.workspaces WHERE id=$1 FOR UPDATE NOWAIT")
        .bind(workspace)
        .fetch_one(&mut *deletion)
        .await
        .expect_err("workspace deletion fence");
    assert_eq!(
        blocked
            .as_database_error()
            .and_then(sqlx::error::DatabaseError::code)
            .as_deref(),
        Some("55P03")
    );
    deletion.rollback().await.expect("release failed contender");
}
