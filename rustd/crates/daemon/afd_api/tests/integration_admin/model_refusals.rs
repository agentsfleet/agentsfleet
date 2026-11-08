//! Real router refusals distinguish missing models from an active default.

use super::{Fixture, MODEL_UPDATE, Method, StatusCode, Value, contains_id, json_body, send};

pub(super) async fn missing(router: &axum::Router, token: &str, id: &str) {
    let response = send(
        router,
        Method::PATCH,
        &format!("/v1/admin/models/{id}"),
        Some(token),
        MODEL_UPDATE,
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        json_body(response)
            .await
            .get("error_code")
            .and_then(Value::as_str),
        Some("UZ-PROVIDER-006")
    );
}

pub(super) async fn in_use(router: &axum::Router, fixture: &Fixture) {
    let response = send(
        router,
        Method::DELETE,
        &format!("/v1/admin/models/{}", fixture.platform_model_row),
        Some(&fixture.token),
        "",
    )
    .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(
        json_body(response)
            .await
            .get("error_code")
            .and_then(Value::as_str),
        Some("UZ-PROVIDER-007")
    );
    let retained = send(
        router,
        Method::GET,
        "/v1/admin/models",
        Some(&fixture.token),
        "",
    )
    .await;
    assert_eq!(retained.status(), StatusCode::OK);
    assert!(contains_id(
        &json_body(retained).await,
        "models",
        &fixture.platform_model_row
    ));
}
