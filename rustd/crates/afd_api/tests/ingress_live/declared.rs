//! Stored custom recipes share the fleet with registry sources.

use super::{
    EVENT_WORKFLOW_RUN, Fixture, Runnable, SIGNING_SECRET, Secret, deliver, harness, json_body,
    signed,
};
use http::{Method, StatusCode};
use serde_json::json;

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_family_never_borrows_another_familys_proof() {
    let fixture = Fixture::create().await;
    fixture.seed(Runnable::Active, Secret::Stored).await;
    let router = fixture.router();
    let body = r#"{"type":"user.created"}"#;
    let timestamp = harness::frozen_unix_seconds().to_string();
    let svix = signed::svix_signature(signed::SVIX_DELIVERY, harness::frozen_unix_seconds(), body);
    let hex = signed::signature(
        afd_webhook::Scheme::BodyHex,
        SIGNING_SECRET,
        body.as_bytes(),
    );
    let svix_headers = signed::svix_headers(signed::SVIX_DELIVERY, &timestamp, &svix);
    let hex_headers = vec![(signed::name("x-acme-signature"), hex.as_str())];
    for (source, headers) in [("acme", &svix_headers), ("clerk", &hex_headers)] {
        let path = format!("/v1/webhooks/{}/{source}", fixture.fleet);
        let refused =
            harness::send_with_headers(&router, Method::POST, &path, None, body, headers).await;
        assert_eq!(refused.status(), StatusCode::UNAUTHORIZED, "{source}");
        assert_eq!(
            json_body(refused).await.get("error_code"),
            Some(&json!("UZ-WH-010"))
        );
    }
    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn two_families_on_one_fleet_each_verify_at_their_own_url() {
    let fixture = Fixture::create().await;
    fixture.seed(Runnable::Active, Secret::Stored).await;
    let router = fixture.router();
    let at = harness::frozen_unix_seconds();
    let body = r#"{"type":"user.created"}"#;
    let signature = signed::svix_signature(signed::SVIX_DELIVERY, at, body);
    let timestamp = at.to_string();
    let headers = signed::svix_headers(signed::SVIX_DELIVERY, &timestamp, &signature);
    let path = format!("/v1/webhooks/{}/clerk", fixture.fleet);
    let first =
        harness::send_with_headers(&router, Method::POST, &path, None, body, &headers).await;
    assert_eq!(first.status(), StatusCode::ACCEPTED);
    assert_eq!(json_body(first).await.get("replayed"), Some(&json!(false)));
    let retry =
        harness::send_with_headers(&router, Method::POST, &path, None, body, &headers).await;
    assert_eq!(retry.status(), StatusCode::ACCEPTED);
    assert_eq!(json_body(retry).await.get("replayed"), Some(&json!(true)));
    assert_eq!(
        deliver(&router, &fixture.fleet, SIGNING_SECRET)
            .await
            .status(),
        StatusCode::ACCEPTED
    );

    let crossed = format!("/v1/webhooks/{}/github", fixture.fleet);
    let mut crossed_headers = headers.clone();
    crossed_headers.push((signed::name(signed::HEADER_EVENT), EVENT_WORKFLOW_RUN));
    let refused = harness::send_with_headers(
        &router,
        Method::POST,
        &crossed,
        None,
        body,
        &crossed_headers,
    )
    .await;
    assert_eq!(refused.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        json_body(refused).await.get("error_code"),
        Some(&json!("UZ-WH-010"))
    );
    fixture.cleanup().await;
}

#[tokio::test]
async fn the_first_trigger_svix_route_is_gone() {
    let router = harness::Fleet::new().router();
    let path = format!("/v1/webhooks/svix/{}", signed::FLEET);
    let response = harness::send_with_headers(&router, Method::POST, &path, None, "{}", &[]).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
