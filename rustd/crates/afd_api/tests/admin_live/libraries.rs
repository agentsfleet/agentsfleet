//! Platform library response variants over the parent live fixture.

use http::{Method, StatusCode, header};
use serde_json::Value;

use super::{contains_id, json_body, send};
use crate::harness::send_with_headers;

pub(super) async fn exercise(router: &axum::Router, token: &str, slug: &str) {
    create_and_list(router, token, slug).await;
    let item = format!("/v1/admin/fleet-libraries/{slug}");
    assert_stale_edit(router, token, &item).await;
    assert_publication_lifecycle(router, token, &item).await;
    assert_missing_library(router, token, &item).await;
}

async fn assert_stale_edit(router: &axum::Router, token: &str, item: &str) {
    let patched = send(
        router,
        Method::PATCH,
        item,
        Some(token),
        r#"{"description":"curated live"}"#,
    )
    .await;
    assert_eq!(patched.status(), StatusCode::OK);
    let etag = patched
        .headers()
        .get(header::ETAG)
        .and_then(|value| value.to_str().ok())
        .expect("a successful patch returns the current etag")
        .to_owned();

    let moved = send(
        router,
        Method::PATCH,
        item,
        Some(token),
        r#"{"description":"moved past the loaded version"}"#,
    )
    .await;
    assert_eq!(moved.status(), StatusCode::OK);
    let stale = send_with_headers(
        router,
        Method::PATCH,
        item,
        Some(token),
        r#"{"description":"stale edit"}"#,
        &[(header::IF_MATCH, &etag)],
    )
    .await;
    assert_eq!(stale.status(), StatusCode::PRECONDITION_FAILED);
}

async fn assert_publication_lifecycle(router: &axum::Router, token: &str, item: &str) {
    patch_status(router, token, item, true, StatusCode::OK).await;
    let protected = send(router, Method::DELETE, item, Some(token), "").await;
    assert_eq!(protected.status(), StatusCode::CONFLICT);
    patch_status(router, token, item, false, StatusCode::OK).await;

    let detached = send(
        router,
        Method::PATCH,
        item,
        Some(token),
        r#"{"source_repo":"another/repository"}"#,
    )
    .await;
    assert_eq!(detached.status(), StatusCode::OK);
    patch_status(router, token, item, true, StatusCode::CONFLICT).await;

    let deleted = send(router, Method::DELETE, item, Some(token), "").await;
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
}

async fn assert_missing_library(router: &axum::Router, token: &str, item: &str) {
    let missing_patch = send(
        router,
        Method::PATCH,
        item,
        Some(token),
        r#"{"description":"missing"}"#,
    )
    .await;
    assert_eq!(missing_patch.status(), StatusCode::NOT_FOUND);
    let missing_delete = send(router, Method::DELETE, item, Some(token), "").await;
    assert_eq!(missing_delete.status(), StatusCode::NOT_FOUND);
}

async fn create_and_list(router: &axum::Router, token: &str, slug: &str) {
    let body = serde_json::json!({
        "source_kind": "upload",
        "source_ref": format!("unit/{slug}"),
        "skill_markdown": format!(
            "---\nname: {slug}\ndescription: Live catalogue fixture\nversion: 1.0.0\n---\nInstructions."
        )
    })
    .to_string();
    let created = send(
        router,
        Method::POST,
        "/v1/admin/fleet-libraries",
        Some(token),
        &body,
    )
    .await;
    assert_eq!(created.status(), StatusCode::CREATED);
    assert_eq!(
        json_body(created).await.get("id").and_then(Value::as_str),
        Some(slug)
    );

    let listed = send(
        router,
        Method::GET,
        "/v1/admin/fleet-libraries",
        Some(token),
        "",
    )
    .await;
    assert_eq!(listed.status(), StatusCode::OK);
    assert!(contains_id(&json_body(listed).await, "entries", slug));
}

async fn patch_status(
    router: &axum::Router,
    token: &str,
    item: &str,
    published: bool,
    expected: StatusCode,
) {
    let response = send(
        router,
        Method::PATCH,
        item,
        Some(token),
        &serde_json::json!({"published": published}).to_string(),
    )
    .await;
    assert_eq!(response.status(), expected);
}
