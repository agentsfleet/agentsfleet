//! Unreviewed or unreadable setup cannot fall back to mutable Markdown/config.

use super::{definition, id};
use crate::{seed::seeded, support::Fixtures};
use afd_fleet_runtime::definition::POLICY_SEMANTICS;

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn missing_native_revision_is_permanent_even_if_old_projection_exists() {
    let fixtures = Fixtures::create_with_queue().await;
    let held = seeded::<0>(&fixtures).await;
    // The seed saves a native revision; deselect it, so the fleet has none.
    sqlx::query("UPDATE fleet.fleets SET head_revision_id = NULL WHERE id = $1::uuid")
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
    let model = Some(afd_credential::provider::PLATFORM_MODEL_REF);
    fixtures
        .append_head(&held.fleet, "{}", "malformed", POLICY_SEMANTICS, model)
        .await;
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
    let stored = serde_json::to_string(&definition()).expect("Definition JSON");
    let model = Some(afd_credential::provider::PLATFORM_MODEL_REF);
    for (semantics, model_ref) in [("previous-policy", model), (POLICY_SEMANTICS, None)] {
        fixtures
            .append_head(&held.fleet, &stored, "invalidated", semantics, model_ref)
            .await;
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
