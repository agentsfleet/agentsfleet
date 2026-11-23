//! A connect whose starter loses the workspace between start and finish.
//!
//! The state proves who started the connect and where; the finish asks again
//! whether that person still holds the workspace, before the nonce is spent or
//! the vendor is asked.

#![cfg(feature = "test-util")]

use super::*;

#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_connect_whose_starter_lost_the_workspace_is_refused_before_the_vendor() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let provider = FakeProvider::answering(&[&slack_answer(&fixture, BOT_TOKEN)]).await;
    let fleet = harness::Fleet::live(
        fixture.database.clone(),
        &fixture.subject,
        afd_auth::scope::ScopeSet::from_scopes(&afd_auth::Scope::ALL),
    )
    .with_owned_workspace(fixture.workspace.clone())
    .with_platform_admin(
        af_core::id::Uuid7::parse(fixture.admin_workspace()).expect("the admin workspace"),
    )
    .with_live_connectors(
        fixture.database.clone(),
        fixture.bus.clone(),
        provider.url(),
    );
    let ownership = fleet.ownership();
    let router = fleet.router();

    let state = start_connect(&router, &fixture, PROVIDER).await;
    ownership.revoke();
    let refused = complete(&router, &fixture, PROVIDER, &state).await;
    let status = refused.status();
    let document = json_body(refused).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{document}");
    assert_eq!(
        document.get("error_code").and_then(Value::as_str),
        Some(error_code::AUTH_FORBIDDEN.as_str())
    );
    assert_eq!(
        provider.exchanges(),
        0,
        "no code is redeemed for a lost workspace"
    );
    assert!(fixture.grant(PROVIDER).await.is_none());

    provider.close();
    fixture.cleanup().await;
}
