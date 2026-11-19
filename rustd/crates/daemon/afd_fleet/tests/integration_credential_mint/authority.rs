//! A runner's credential authority survives neither widening nor cancellation.

use super::*;
use crate::requests;

async fn mint_static(
    fixtures: &Fixtures,
    owner: &Bound,
) -> afd_fleet::error::Result<afd_credential::credential::Minted> {
    fixtures
        .plane()
        .mint(
            &owner.runner,
            &requests::mint(owner.lease.as_str(), CONNECTOR_STATIC),
            fixtures.enrolled_at(),
        )
        .await
}

pub(super) fn config(credentials: &[&str]) -> String {
    let mut config: serde_json::Value =
        serde_json::from_str(af_fleet_runtime::test_util::FIXTURE_CONFIG)
            .expect("canonical fixture config");
    runtime(&mut config).insert("credentials".to_owned(), serde_json::json!(credentials));
    config.to_string()
}

fn runtime(config: &mut serde_json::Value) -> &mut serde_json::Map<String, serde_json::Value> {
    config
        .get_mut("x-agentsfleet")
        .and_then(serde_json::Value::as_object_mut)
        .expect("canonical runtime object")
}

async fn seed_static(fixtures: &Fixtures, owner: &Bound) {
    seed_handle(
        fixtures,
        &owner.workspace,
        CONNECTOR_STATIC,
        &format!(r#"{{"integration":"static","token":"{OWNER_TOKEN}"}}"#),
    )
    .await;
}

#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn runner_mint_requires_the_issued_and_current_credential_declarations() {
    let fixtures = Fixtures::create().await;
    let owner = bound(&fixtures, &config(&["static"])).await;
    seed_static(&fixtures, &owner).await;
    assert_eq!(
        mint_static(&fixtures, &owner)
            .await
            .expect("declared credential")
            .token
            .as_str(),
        OWNER_TOKEN
    );
    set_config(&fixtures, &owner.fleet, &config(&[])).await;
    assert_eq!(
        mint_static(&fixtures, &owner)
            .await
            .expect_err("current setup withdrew authority")
            .code(),
        error_code::CRED_INTEGRATION_NOT_CONNECTED
    );
    let unassigned = bound(&fixtures, &config(&[])).await;
    seed_static(&fixtures, &unassigned).await;
    set_config(&fixtures, &unassigned.fleet, &config(&["static"])).await;
    assert_eq!(
        mint_static(&fixtures, &unassigned)
            .await
            .expect_err("a new head cannot widen a lease")
            .code(),
        error_code::CRED_INTEGRATION_NOT_CONNECTED
    );
    fixtures.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn approved_connectors_and_unreadable_setup_do_not_bypass_declarations() {
    let fixtures = Fixtures::create().await;
    let owner = bound(&fixtures, &config(&[])).await;
    seed_grant(&fixtures, &owner.fleet, CONNECTOR_GITHUB, STATUS_APPROVED).await;
    seed_handle(
        &fixtures,
        &owner.workspace,
        CONNECTOR_GITHUB,
        r#"{"integration":"static","token":"private-token"}"#,
    )
    .await;
    let refusal = fixtures
        .plane()
        .mint(
            &owner.runner,
            &requests::mint(owner.lease.as_str(), CONNECTOR_GITHUB),
            fixtures.enrolled_at(),
        )
        .await
        .expect_err("a grant cannot add a credential to the assignment");
    assert_eq!(refusal.code(), error_code::CRED_INTEGRATION_NOT_CONNECTED);
    let owner = bound(&fixtures, &config(&["static"])).await;
    seed_static(&fixtures, &owner).await;
    fixtures
        .corrupt_native_config(&owner.fleet, r#"{"x-agentsfleet":{"credentials":42}}"#)
        .await;
    assert_eq!(
        mint_static(&fixtures, &owner)
            .await
            .expect_err("unreadable setup withholds")
            .code(),
        error_code::CRED_INTEGRATION_NOT_CONNECTED
    );
    fixtures.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn cancellation_committed_before_a_new_mint_withholds_credentials() {
    let fixtures = Fixtures::create().await;
    let owner = bound(&fixtures, &config(&["static"])).await;
    seed_static(&fixtures, &owner).await;
    mint_static(&fixtures, &owner)
        .await
        .expect("active control");
    sqlx::query("UPDATE runner.leases SET cancel_requested_at=clock_timestamp() WHERE id=$1")
        .bind(&owner.lease)
        .execute(&mut *fixtures.database.acquire().await.expect("connection"))
        .await
        .expect("durable cancellation");
    assert_eq!(
        mint_static(&fixtures, &owner)
            .await
            .expect_err("cancellation removes authority")
            .code(),
        error_code::RUN_LEASE_NOT_FOUND
    );
    fixtures.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_lease_without_its_issued_setup_cannot_inherit_the_current_credentials() {
    let fixtures = Fixtures::create().await;
    let owner = bound(&fixtures, &config(&["static"])).await;
    seed_static(&fixtures, &owner).await;
    mint_static(&fixtures, &owner)
        .await
        .expect("issued control");
    sqlx::query("UPDATE runner.leases SET setup_revision_id=NULL WHERE id=$1")
        .bind(&owner.lease)
        .execute(&mut *fixtures.database.acquire().await.expect("connection"))
        .await
        .expect("legacy lease without assignment witness");
    assert_eq!(
        mint_static(&fixtures, &owner)
            .await
            .expect_err("missing authority withholds")
            .code(),
        error_code::CRED_INTEGRATION_NOT_CONNECTED
    );
    fixtures.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn editing_repository_scope_cannot_widen_an_existing_runners_mint() {
    let fixtures = Fixtures::create().await;
    let mut configuration: serde_json::Value =
        serde_json::from_str(&config(&["github"])).expect("canonical configuration");
    runtime(&mut configuration).insert(
        "repositories".to_owned(),
        serde_json::json!(["acme/payments"]),
    );
    runtime(&mut configuration).insert("repository_access".to_owned(), serde_json::json!("read"));
    let owner = bound(&fixtures, &configuration.to_string()).await;
    seed_grant(&fixtures, &owner.fleet, CONNECTOR_GITHUB, STATUS_APPROVED).await;
    seed_handle(
        &fixtures,
        &owner.workspace,
        CONNECTOR_GITHUB,
        &format!(r#"{{"integration":"static","token":"{OWNER_TOKEN}"}}"#),
    )
    .await;
    let plane = fixtures.plane();
    let request = requests::mint(owner.lease.as_str(), CONNECTOR_GITHUB);
    let control = plane
        .mint(&owner.runner, &request, fixtures.enrolled_at())
        .await
        .expect("issued repository scope");
    assert_eq!(control.token.as_str(), OWNER_TOKEN);
    runtime(&mut configuration).insert(
        "repositories".to_owned(),
        serde_json::json!(["acme/payments", "acme/other"]),
    );
    set_config(&fixtures, &owner.fleet, &configuration.to_string()).await;
    assert_eq!(
        plane
            .mint(&owner.runner, &request, fixtures.enrolled_at())
            .await
            .expect_err("new repository scope needs a new assignment")
            .code(),
        error_code::CRED_INTEGRATION_NOT_CONNECTED
    );
    fixtures.cleanup().await;
}
