//! Unreviewed or unreadable setup cannot fall back to mutable Markdown/config.

use super::{definition, id};
use crate::{seed::seeded, support::Fixtures};

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn missing_native_revision_is_permanent_even_if_old_projection_exists() {
    let fixtures = Fixtures::create_with_queue().await;
    let held = seeded::<0>(&fixtures).await;
    // The seed saves a native revision; deselect it, so only the old
    // `config_json` projection is left to fall back to.
    sqlx::query("UPDATE core.fleets SET current_definition_revision_id=NULL WHERE id=$1::uuid")
        .bind(&held.fleet)
        .execute(&mut *fixtures.database.acquire().await.expect("connection"))
        .await
        .expect("deselect the native revision");
    let failure = fixtures
        .leases()
        .installed(&id(&held.fleet))
        .await
        .expect_err("no native authority");
    assert!(failure.is_config_permanent());
    assert_eq!(failure.code(), afd_core::error_code::AUTHORING_POLICY);
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn malformed_native_definition_refuses_instead_of_using_a_valid_projection() {
    let fixtures = Fixtures::create_with_queue().await;
    let held = seeded::<0>(&fixtures).await;
    fixtures.native_definition(&held.fleet, &definition()).await;
    sqlx::query(
        "UPDATE core.fleet_definition_revisions SET definition='{}'::jsonb WHERE fleet_id=$1::uuid",
    )
    .bind(&held.fleet)
    .execute(&mut *fixtures.database.acquire().await.expect("connection"))
    .await
    .expect("inject malformed persisted document");
    let failure = fixtures
        .leases()
        .installed(&id(&held.fleet))
        .await
        .expect_err("invalid Definition");
    assert!(failure.is_config_permanent());
    assert!(!failure.is_datastore_unavailable());
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn old_policy_or_missing_model_cannot_execute() {
    let fixtures = Fixtures::create_with_queue().await;
    let held = seeded::<0>(&fixtures).await;
    fixtures.native_definition(&held.fleet, &definition()).await;
    for statement in [
        "UPDATE core.fleet_definition_revisions SET policy_semantics='previous-policy' WHERE fleet_id=$1::uuid",
        "UPDATE core.fleet_definition_revisions SET policy_semantics=$2,runtime_model_ref=NULL WHERE fleet_id=$1::uuid",
    ] {
        let mut query = sqlx::query(statement).bind(&held.fleet);
        if statement.contains("$2") {
            query = query.bind(afd_fleet_runtime::definition::POLICY_SEMANTICS);
        }
        query
            .execute(&mut *fixtures.database.acquire().await.expect("connection"))
            .await
            .expect("invalidate setup");
        assert!(
            fixtures
                .leases()
                .installed(&id(&held.fleet))
                .await
                .expect_err("setup must be re-reviewed")
                .is_config_permanent()
        );
    }
}
