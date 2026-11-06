//! Real HTTP page boundaries, refusals and full catalogue consumers.
use super::{Method, StatusCode, Value, json_body, send};
use axum::Router;

pub(super) async fn all(router: &Router, token: &str) -> Vec<Value> {
    let mut models = Vec::new();
    let mut cursor = None;
    let mut seen = std::collections::HashSet::new();
    loop {
        let path = cursor.as_ref().map_or_else(
            || "/v1/admin/models".to_owned(),
            |cursor| format!("/v1/admin/models?starting_after={cursor}"),
        );
        let response = send(router, Method::GET, &path, Some(token), "").await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = json_body(response).await;
        models.extend(
            body["models"]
                .as_array()
                .expect("model page")
                .iter()
                .cloned(),
        );
        cursor = body["next_cursor"].as_str().map(str::to_owned);
        let Some(next) = &cursor else {
            return models;
        };
        assert!(seen.insert(next.clone()), "no repeated cursor");
    }
}

pub(super) async fn check(router: &Router, token: &str) {
    invalid_pages(router, token).await;
    let first = send(
        router,
        Method::GET,
        "/v1/admin/models?limit=1",
        Some(token),
        "",
    )
    .await;
    assert_eq!(first.status(), StatusCode::OK);
    let body = json_body(first).await;
    assert_eq!(body["models"].as_array().expect("one row").len(), 1);
    let next = body["next_cursor"]
        .as_str()
        .expect("fixture has multiple rows");
    let path = format!("/v1/admin/models?limit=1&starting_after={next}");
    let response = send(router, Method::GET, &path, Some(token), "").await;
    assert_eq!(response.status(), StatusCode::OK);
    let second = json_body(response).await;
    assert_ne!(body["models"][0]["id"], second["models"][0]["id"]);
    check_replay(router, token, &path, next, &second).await;
}

async fn invalid_pages(router: &Router, token: &str) {
    for query in [
        "limit=0",
        "limit=101",
        "limit=no",
        "starting_after=malformed",
    ] {
        let response = send(
            router,
            Method::GET,
            &format!("/v1/admin/models?{query}"),
            Some(token),
            "",
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{query}");
        let body = json_body(response).await;
        assert_eq!(body["error_code"], "UZ-REQ-001");
        if query.starts_with("limit=") {
            assert_eq!(
                body["detail"],
                afd_api_operator::test_util::MODEL_PAGE_LIMIT_DETAIL
            );
        }
    }
}

async fn check_replay(router: &Router, token: &str, path: &str, next: &str, second: &Value) {
    let replay = json_body(send(router, Method::GET, path, Some(token), "").await).await;
    assert_eq!(second["models"], replay["models"]);
    let refused = send(
        router,
        Method::GET,
        &format!("/v1/admin/models?limit=2&starting_after={next}"),
        Some(token),
        "",
    )
    .await;
    assert_eq!(refused.status(), StatusCode::BAD_REQUEST);
    assert_eq!(json_body(refused).await["error_code"], "UZ-REQ-001");
}
