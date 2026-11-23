use http::Method;

use super::super::{fixture::Fixture, mint, send};
use super::{Case, TOKEN, check, json};

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn per_fleet_webhook_overrides_require_the_ingress_signing_field() {
    for body in [
        json!({"api_key":TOKEN}),
        json!({"webhook_secret":""}),
        json!({"webhook_secret":42}),
        json!({"webhook_secret":null}),
    ] {
        let case = Case::create(json!({"triggers":[{"type":"webhook","source":"github",
            "credential_name":"fleet-signing"}]}))
        .await;
        case.store("fleet-signing", body).await;
        check(&case.read().await, "credential", "fleet-signing", "blocked");
        case.cleanup().await;
    }
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn per_fleet_webhooks_need_no_outbound_github_app_handle() {
    let case = Case::create(json!({"triggers":[{"type":"webhook","source":"github",
        "credential_name":"fleet-signing"}]}))
    .await;
    case.store("fleet-signing", json!({"webhook_secret":TOKEN}))
        .await;
    let review = case.read().await;
    check(&review, "credential", "fleet-signing", "ready");
    assert!(
        review["readiness"]
            .as_array()
            .expect("checks")
            .iter()
            .all(|check| check["kind"] != "connection" && check["kind"] != "grant")
    );
    case.cleanup().await;
}

/// A source no route could verify never becomes a draft, so there is no
/// candidate a stored signing field could make ready.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn an_unverifiable_source_is_refused_before_it_can_become_ready() {
    let base = Fixture::create().await;
    let mut request = base.request(&mint());
    request["source"]["definition"]["config"]["x-agentsfleet"]["triggers"] =
        json!([{"type":"webhook","source":"unknown-hook"}]);
    let response = send(
        &base.router(),
        Method::POST,
        &base.collection(),
        Some(&base.token),
        &request.to_string(),
    )
    .await;
    assert!(
        response.status().is_client_error(),
        "an unverifiable webhook source is the author's to fix: {}",
        response.status()
    );
    base.cleanup().await;
}
