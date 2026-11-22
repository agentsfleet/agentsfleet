//! Public replay-key and capability boundaries run before private mutation.

use super::{fixture::Fixture, json_body, mint, send, send_with_headers};
use afd_auth::Scope;
use http::{Method, StatusCode, header::HeaderName};
use serde_json::json;

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn body_keys_are_required_and_header_keys_are_refused() {
    let f = Fixture::create().await;
    let router = f.router();
    let operation = mint();
    let mut body = f.request(&operation);
    let key = [(
        HeaderName::from_static("idempotency-key"),
        operation.as_str(),
    )];
    let response = send_with_headers(
        &router,
        Method::POST,
        &f.collection(),
        Some(&f.token),
        &body.to_string(),
        &key,
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    body.as_object_mut().expect("object").remove("operation_id");
    assert_eq!(
        send(
            &router,
            Method::POST,
            &f.collection(),
            Some(&f.token),
            &body.to_string()
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST
    );
    let read_only = f.services(&[Scope::FleetRead]).router();
    let response = send(&read_only, Method::GET, &f.collection(), Some(&f.token), "").await;
    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "private drafts require fleet:write even for reads"
    );
    let listed = send(&router, Method::GET, &f.collection(), Some(&f.token), "").await;
    assert_eq!(
        json_body(listed).await["items"],
        json!([]),
        "refused writes allocate nothing"
    );
    f.cleanup().await;
}

/// A field other than the message outside its bounds earns the setup shape
/// sentence, not the message ceiling's, and allocates no draft.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_setup_model_outside_its_bounds_is_refused_with_the_shape_sentence() {
    let f = Fixture::create().await;
    let router = f.router();
    for model in [String::new(), "model\u{0}ref".to_owned()] {
        let mut body = f.request(&mint());
        body["setup_model_ref"] = json!(model);
        let response = send(
            &router,
            Method::POST,
            &f.collection(),
            Some(&f.token),
            &body.to_string(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{model:?}");
        let problem = json_body(response).await;
        assert_eq!(problem["error_code"], "UZ-REQ-001", "{problem}");
        assert_eq!(
            problem["detail"], "Setup input violates the supported shape or bounds",
            "{problem}"
        );
    }
    let listed = send(&router, Method::GET, &f.collection(), Some(&f.token), "").await;
    assert_eq!(json_body(listed).await["items"], json!([]));
    f.cleanup().await;
}
