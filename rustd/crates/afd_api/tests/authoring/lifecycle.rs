//! Both clients observe durable queue acceptance and explicit native revisions.

use super::{fixture::Fixture, json_body, mint, send, send_with_headers};
use afd_core::error_code;
use http::{Method, StatusCode, header};
use serde_json::{Value, json};

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn private_draft_http_lifecycle_replays_stops_edits_and_purges() {
    let f = Fixture::create().await;
    let router = f.router();
    let request = f.request(&mint()).to_string();
    let created = send(
        &router,
        Method::POST,
        &f.collection(),
        Some(&f.token),
        &request,
    )
    .await;
    let (item, tag) = created_headers(&created);
    let created = json_body(created).await;
    assert_eq!(
        created.pointer("/candidate/definition/instructions_md"),
        Some(&json!("Investigate.\n"))
    );
    assert_eq!(created.pointer("/accepted/replayed"), Some(&json!(false)));
    let replay = send(
        &router,
        Method::POST,
        &f.collection(),
        Some(&f.token),
        &request,
    )
    .await;
    assert_eq!(replay.status(), StatusCode::CREATED);
    assert_eq!(
        json_body(replay).await.pointer("/accepted/replayed"),
        Some(&json!(true))
    );
    stop_then_edit(&f, &router, &item, &tag, &created).await;
    let listed = send(&router, Method::GET, &f.collection(), Some(&f.token), "").await;
    assert_eq!(
        json_body(listed).await.pointer("/items/0/id"),
        created.get("id")
    );
    purge_and_refuse_replay(&f, &router, &item, &request).await;
    f.cleanup().await;
}

async fn stop_then_edit(
    f: &Fixture,
    router: &axum::Router,
    item: &str,
    tag: &str,
    created: &Value,
) {
    let patch = json!({"title":"Checkout operations"}).to_string();
    let missing = send(router, Method::PATCH, item, Some(&f.token), &patch).await;
    assert_eq!(missing.status(), StatusCode::PRECONDITION_REQUIRED);
    let busy = send_with_headers(
        router,
        Method::PATCH,
        item,
        Some(&f.token),
        &patch,
        &[(header::IF_MATCH, tag)],
    )
    .await;
    assert_eq!(busy.status(), StatusCode::CONFLICT);
    assert_eq!(json_body(busy).await["current_state"], "authoring");
    let stopped = send(
        router,
        Method::POST,
        &format!("{item}/cancellations"),
        Some(&f.token),
        r#"{"through_sequence":"1"}"#,
    )
    .await;
    assert_eq!(stopped.status(), StatusCode::OK);
    assert_conditional_edit(f, router, item, tag, &patch, created).await;
}

async fn purge_and_refuse_replay(f: &Fixture, router: &axum::Router, item: &str, request: &str) {
    assert_eq!(
        send(router, Method::DELETE, item, Some(&f.token), "")
            .await
            .status(),
        StatusCode::NO_CONTENT
    );
    let gone = send(
        router,
        Method::POST,
        &f.collection(),
        Some(&f.token),
        request,
    )
    .await;
    assert_eq!(gone.status(), StatusCode::GONE);
    assert_eq!(
        json_body(gone).await["error_code"],
        error_code::AUTHORING_GONE.as_str()
    );
    assert_eq!(
        send(router, Method::GET, item, Some(&f.token), "")
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
}

async fn assert_conditional_edit(
    f: &Fixture,
    router: &axum::Router,
    item: &str,
    tag: &str,
    patch: &str,
    created: &Value,
) {
    let current = send(router, Method::GET, item, Some(&f.token), "").await;
    let fresh_tag = current.headers()[header::ETAG]
        .to_str()
        .expect("post-stop tag")
        .to_owned();
    let changed = send_with_headers(
        router,
        Method::PATCH,
        item,
        Some(&f.token),
        patch,
        &[(header::IF_MATCH, &fresh_tag)],
    )
    .await;
    assert_eq!(changed.status(), StatusCode::OK);
    let changed = json_body(changed).await;
    assert_eq!(changed["title"], "Checkout operations");
    assert_eq!(changed.get("candidate"), created.get("candidate"));
    let stale = send_with_headers(
        router,
        Method::PATCH,
        item,
        Some(&f.token),
        patch,
        &[(header::IF_MATCH, tag)],
    )
    .await;
    assert_eq!(stale.status(), StatusCode::PRECONDITION_FAILED);
    assert_eq!(json_body(stale).await.get("etag"), changed.get("etag"));
}

fn created_headers(created: &axum::response::Response) -> (String, String) {
    assert_eq!(created.status(), StatusCode::CREATED);
    assert_eq!(
        created.headers()[header::CACHE_CONTROL],
        "private, no-store"
    );
    let item = created.headers()[header::LOCATION]
        .to_str()
        .expect("item location")
        .to_owned();
    let tag = created.headers()[header::ETAG]
        .to_str()
        .expect("strong tag")
        .to_owned();
    (item, tag)
}
