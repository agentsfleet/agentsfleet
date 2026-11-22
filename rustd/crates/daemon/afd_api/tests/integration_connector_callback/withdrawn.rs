//! A connect whose app credentials are withdrawn between start and finish.
//!
//! The start reads the deployment's app to build the consent URL, and the
//! finish reads it again to redeem the code. An operator can remove it in
//! between, and the finish must then refuse as unconfigured without reaching
//! the vendor or sealing anything.

#![cfg(feature = "test-util")]

use super::*;

#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_connect_whose_app_was_withdrawn_finishes_as_not_configured() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let provider = FakeProvider::answering(&[&slack_answer(&fixture, BOT_TOKEN)]).await;
    let router = fixture.router(&provider);

    let state = start_connect(&router, &fixture, PROVIDER).await;
    fixture.withdraw_app(PROVIDER).await;
    let refused = complete(&router, &fixture, PROVIDER, &state).await;
    let status = refused.status();
    let document = json_body(refused).await;
    assert_eq!(
        document.get("error_code").and_then(Value::as_str),
        Some(error_code::CONNECTOR_NOT_CONFIGURED.as_str()),
        "{status} {document}"
    );
    assert_eq!(provider.exchanges(), 0, "no app, so no code is redeemed");
    assert!(fixture.grant(PROVIDER).await.is_none());

    provider.close();
    fixture.cleanup().await;
}

/// A deployment whose app for a provider is gone cannot start that connect:
/// no consent URL is composed and no state is remembered.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_connect_started_with_no_app_answers_not_configured() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let provider = FakeProvider::answering(&[&slack_answer(&fixture, BOT_TOKEN)]).await;
    let router = fixture.router(&provider);
    fixture.withdraw_app(PROVIDER).await;

    let path = format!(
        "/v1/workspaces/{}/connectors/{}/connect",
        fixture.workspace.as_str(),
        PROVIDER.id()
    );
    let refused = send(&router, Method::POST, &path, Some(&fixture.token), "").await;
    let status = refused.status();
    let document = json_body(refused).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{document}");
    assert_eq!(
        document.get("error_code").and_then(Value::as_str),
        Some(error_code::CONNECTOR_NOT_CONFIGURED.as_str())
    );
    assert!(document.get("install_url").is_none());
    assert_eq!(provider.exchanges(), 0, "nothing reaches the vendor");

    provider.close();
    fixture.cleanup().await;
}
