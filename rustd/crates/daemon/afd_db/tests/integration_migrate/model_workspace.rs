//! Fresh-schema model ownership survives deletion of its credential workspace.

use af_core::id::Uuid7;
use afd_db::test_util::{TenantSeed, WorkspaceSeed, mint_id};

use super::*;

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn deleting_a_credential_workspace_retains_the_model_and_selection() {
    let (database, db) = migrated().await;
    let mut connection = db.acquire().await.expect("workspace fixture connection");
    let (tenant, workspace, entry) = selected_model(&mut connection).await;
    WorkspaceSeed::new(&mint_id(), &tenant)
        .insert(&mut *connection)
        .await
        .expect("another workspace remains available");

    sqlx::query("DELETE FROM identity.workspaces WHERE id=$1")
        .bind(&workspace)
        .execute(&mut *connection)
        .await
        .expect("workspace deletion retains model history");
    let retained: (Option<Uuid7>, Uuid7) = sqlx::query_as(
        "SELECT m.workspace_id, s.tenant_model_id FROM model.tenant_models m
         JOIN model.tenant_selections s ON s.tenant_model_id=m.id WHERE m.id=$1",
    )
    .bind(&entry)
    .fetch_one(&mut *connection)
    .await
    .expect("the original model and selection remain readable");
    assert_eq!(
        retained,
        (None, entry),
        "a removed credential source never retargets to another workspace"
    );
    drop(connection);
    db.close().await;
    database.cleanup().await;
}

async fn selected_model(connection: &mut PgConnection) -> (Uuid7, Uuid7, Uuid7) {
    let (tenant, workspace, entry) = (mint_id(), mint_id(), mint_id());
    TenantSeed::new(&tenant)
        .insert(&mut *connection)
        .await
        .expect("the account seeds");
    WorkspaceSeed::new(&workspace, &tenant)
        .insert(&mut *connection)
        .await
        .expect("the credential workspace seeds");
    sqlx::query(
        "INSERT INTO model.tenant_models
         (id, tenant_id, workspace_id, model_name, secret_ref, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, now(), now())",
    )
    .bind(&entry)
    .bind(&tenant)
    .bind(&workspace)
    .bind("model")
    .bind("key")
    .execute(&mut *connection)
    .await
    .expect("a model stores its explicit credential workspace");
    sqlx::query(
        "INSERT INTO model.tenant_selections (tenant_id, tenant_model_id, created_at, updated_at)
         VALUES ($1, $2, now(), now())",
    )
    .bind(&tenant)
    .bind(&entry)
    .execute(&mut *connection)
    .await
    .expect("the account selects its model");
    (tenant, workspace, entry)
}
