//! Invalid authority and provider/system overrides cannot reach the physical transport.

use super::super::{json_body, send};
use super::fixture::Model;
use http::{Method, StatusCode};
use serde_json::{Value, json};
use std::sync::atomic::Ordering;

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn person_credentials_forged_capabilities_and_provider_overrides_leave_no_intent() {
    let model = Model::create().await;
    let r = &model.running;
    refuse_person(&model).await;
    let mut forged = model.request.clone();
    *forged.get_mut("capability_token").expect("capability") = Value::String("b".repeat(43));
    let response = send(
        &r.router,
        Method::POST,
        &model.path,
        Some(&r.token),
        &forged.to_string(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(
        json_body(response)
            .await
            .get("error_code")
            .and_then(Value::as_str),
        Some("UZ-AUTHOR-012")
    );
    for (field, value) in [
        ("provider", json!("openai")),
        ("base_url", json!("https://attacker.test")),
        ("api_key", json!("key")),
        ("system", json!("override")),
        ("tools", json!([])),
    ] {
        let mut invalid = model.request.clone();
        invalid
            .as_object_mut()
            .expect("model body")
            .insert(field.into(), value);
        let response = send(
            &r.router,
            Method::POST,
            &model.path,
            Some(&r.token),
            &invalid.to_string(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{field}");
    }
    assert_eq!(model.provider.effects.load(Ordering::SeqCst), 0);
    assert_eq!(model.receipts().await, 0);
    model.running.f.cleanup().await;
}

async fn refuse_person(model: &Model) {
    let r = &model.running;
    let response = send(
        &r.router,
        Method::POST,
        &model.path,
        Some(&r.f.token),
        &model.request.to_string(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
