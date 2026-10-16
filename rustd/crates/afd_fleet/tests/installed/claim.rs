//! The selected Definition wins over divergent old projection columns.

use super::{definition, id};
use crate::{seed::seeded, support::Fixtures};
use afd_fleet::lease::FRESH_CONTEXT;

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn native_instructions_and_model_survive_the_claim_without_markdown_parsing() {
    let fixtures = Fixtures::create_with_queue().await;
    let held = seeded::<0>(&fixtures).await;
    let definition = definition();
    fixtures.native_definition(&held.fleet, &definition).await;
    sqlx::query("UPDATE core.fleets SET config_json='{}'::jsonb WHERE id=$1::uuid")
        .bind(&held.fleet)
        .execute(
            &mut *fixtures
                .database
                .acquire()
                .await
                .expect("projection connection"),
        )
        .await
        .expect("diverge non-authoritative projection");
    let installed = fixtures
        .leases()
        .installed(&id(&held.fleet))
        .await
        .expect("claim read")
        .expect("runnable");
    assert_eq!(installed.instructions, definition.instructions_md());
    assert_eq!(&installed.config, definition.runtime());
    assert_eq!(installed.context_json, FRESH_CONTEXT);
    assert_eq!(
        installed.runtime_model_ref,
        afd_credential::provider::PLATFORM_MODEL_REF
    );
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn stopped_and_missing_fleets_do_not_issue_work() {
    let fixtures = Fixtures::create_with_queue().await;
    let held = seeded::<0>(&fixtures).await;
    fixtures.native_definition(&held.fleet, &definition()).await;
    sqlx::query("UPDATE core.fleets SET status='stopped' WHERE id=$1::uuid")
        .bind(&held.fleet)
        .execute(&mut *fixtures.database.acquire().await.expect("connection"))
        .await
        .expect("stop owned fleet");
    assert!(
        fixtures
            .leases()
            .installed(&id(&held.fleet))
            .await
            .expect("stopped read")
            .is_none()
    );
    let absent = afd_db::test_util::mint_id();
    assert!(
        fixtures
            .leases()
            .installed(&id(&absent))
            .await
            .expect("absent read")
            .is_none()
    );
}
