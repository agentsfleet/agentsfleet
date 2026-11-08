//! Deferred ownership follows final rows while immutable-table grants stay narrow.

use af_core::id::Uuid7;
use afd_admin::DeleteModel;
use afd_db::test_util::mint_id;
use sqlx::{Acquire as _, PgConnection};

use super::fixture::{NOW, Scenario, read};

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn deferred_owner_checks_allow_clearing_or_deleting_an_intermediate_selection() {
    let first = Scenario::create(None).await;
    let second = Scenario::create(None).await;
    let mut connection = first.database.acquire().await.expect("API connection");
    let mut transaction = connection.begin().await.expect("deferred changes");
    point_at_other_model(&mut transaction, &first.model.id, &second.model.id).await;
    sqlx::query("UPDATE model.platform_models SET decision_revision_id=NULL,decision_enabled=false WHERE id=$1")
        .bind(&first.model.id).execute(&mut *transaction).await.expect("clear intermediate selection");
    transaction
        .commit()
        .await
        .expect("final null pointer is valid");
    let cleared = read(&first.models, &first.model.id).await;
    assert!(cleared.details.decision.is_none());
    let mut transaction = connection.begin().await.expect("deferred deletion");
    point_at_other_model(&mut transaction, &first.model.id, &second.model.id).await;
    sqlx::query("DELETE FROM model.platform_models WHERE id=$1")
        .bind(&first.model.id)
        .execute(&mut *transaction)
        .await
        .expect("delete model and owned revisions");
    transaction
        .commit()
        .await
        .expect("deleted model has no live pointer");
    assert_eq!(
        first
            .models
            .delete(&first.model.id)
            .await
            .expect("deleted identity"),
        DeleteModel::NotFound
    );
    drop(connection);
    first.database.close().await;
    second.finish().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn immediate_checks_refuse_missing_revisions_without_widening_api_privileges() {
    let scenario = Scenario::create(None).await;
    let mut connection = scenario.database.acquire().await.expect("API connection");
    let privileges: (bool, bool) = sqlx::query_as("SELECT has_table_privilege(current_user,'model.decision_revisions','UPDATE'),has_function_privilege(current_user,'model.enforce_decision_revision_owner()','EXECUTE')")
        .fetch_one(&mut *connection).await.expect("effective API privileges");
    assert_eq!(
        privileges,
        (false, false),
        "immutable rows and privileged trigger stay closed"
    );
    let before = read(&scenario.models, &scenario.model.id).await;
    assert!(
        scenario
            .models
            .update(&scenario.model.id, before.details.clone(), NOW)
            .await
            .expect("valid API writer passes ownership check")
    );
    let mut transaction = connection.begin().await.expect("immediate check");
    sqlx::query("SET CONSTRAINTS model.fk_platform_models_decision_revision_owner IMMEDIATE")
        .execute(&mut *transaction)
        .await
        .expect("immediate mode");
    let rejected =
        sqlx::query("UPDATE model.platform_models SET decision_revision_id=$2 WHERE id=$1")
            .bind(&scenario.model.id)
            .bind(mint_id())
            .execute(&mut *transaction)
            .await
            .expect_err("missing revision is rejected at the statement");
    assert!(afd_db::constraint::violates_foreign_key(
        &rejected,
        "fk_platform_models_decision_revision_owner"
    ));
    transaction
        .rollback()
        .await
        .expect("discard failed selection");
    assert_eq!(read(&scenario.models, &scenario.model.id).await, before);
    drop(connection);
    scenario.finish().await;
}

async fn point_at_other_model(connection: &mut PgConnection, model: &Uuid7, other: &Uuid7) {
    sqlx::query("UPDATE model.platform_models SET decision_revision_id=(SELECT decision_revision_id FROM model.platform_models WHERE id=$2) WHERE id=$1")
        .bind(model).bind(other).execute(connection).await.expect("deferred intermediate pointer");
}
