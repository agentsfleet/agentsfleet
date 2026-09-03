//! What the inbox listing reads off a query string, over live Postgres.
//!
//! Split from `integration_workspace_approvals.rs` at the seam the file cap
//! draws and the concerns already agreed with: that file proves the lifecycle
//! of one gate, read to answered. This one proves the SHAPE of a read — which
//! rows a filter selects, where a page resumes, and which parameters are
//! refused rather than quietly ignored.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use crate::harness;
use crate::integration_workspace_approvals::{Fixture, LISTING_SUBJECT};

use afd_auth::scope::{Scope, ScopeSet};
use http::{Method, StatusCode};
use serde_json::Value;

use self::harness::{Fleet, json_body, send};

/// The inbox reads the five parameters the dashboard sends, and pages.
///
/// Before this the handler took no query at all: it passed the default filter,
/// no cursor and a fixed size, then answered `next_cursor: null` however many
/// rows were waiting. A dashboard sending `status`, `fleet_id`, `gate_kind`,
/// `limit` and `cursor` had every one of them ignored, which reads as a filter
/// that silently does nothing and a queue that stops at its first page.
#[tokio::test]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn the_inbox_narrows_by_filter_and_pages_by_cursor() {
    let fixture = Fixture::create_as(LISTING_SUBJECT).await;
    fixture.seed().await;
    let second = fixture.seed_second_gate().await;
    let queue = harness::connect_redis().await;
    let router = Fleet::live(
        fixture.database.clone(),
        LISTING_SUBJECT,
        ScopeSet::from_scopes(&Scope::ALL),
    )
    .with_owned_workspace(fixture.workspace.clone())
    .with_approval_queue(fixture.database.clone(), queue)
    .router();
    let collection = format!("/v1/workspaces/{}/approvals", fixture.workspace.as_str());

    assert_filters_narrow(&router, &fixture, &collection, &second).await;
    assert_the_page_resumes(&router, &fixture, &collection, &second).await;
    assert_unreadable_parameters_refuse(&router, &fixture, &collection).await;

    fixture.cleanup().await;
}

/// Each filter selects, and a status nothing holds selects nothing.
async fn assert_filters_narrow(
    router: &axum::Router,
    fixture: &Fixture,
    collection: &str,
    second: &str,
) {
    let ids = |page: &Value| -> Vec<String> {
        page.get("items")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|item| item.get("gate_id").and_then(Value::as_str))
            .map(str::to_owned)
            .collect()
    };

    let all = read(router, &fixture.token, collection, "").await;
    assert_eq!(ids(&all).len(), 2, "both pending gates are waiting: {all}");

    let by_kind = read(router, &fixture.token, collection, "?gate_kind=spend").await;
    assert_eq!(ids(&by_kind), vec![second.to_owned()], "{by_kind}");

    let by_fleet = read(
        router,
        &fixture.token,
        collection,
        &format!("?fleet_id={}", fixture.fleet),
    )
    .await;
    assert_eq!(ids(&by_fleet).len(), 2, "{by_fleet}");

    // Both gates are pending, so a resolved status selects none of them. An
    // ignored parameter would answer the pending page here and look correct.
    let resolved = read(router, &fixture.token, collection, "?status=approved").await;
    assert!(ids(&resolved).is_empty(), "{resolved}");

    let explicit_pending = read(router, &fixture.token, collection, "?status=pending").await;
    assert_eq!(ids(&explicit_pending).len(), 2, "{explicit_pending}");
}

/// A full page hands back a cursor, and that cursor resumes after it.
async fn assert_the_page_resumes(
    router: &axum::Router,
    fixture: &Fixture,
    collection: &str,
    second: &str,
) {
    let first = read(router, &fixture.token, collection, "?limit=1").await;
    assert_eq!(
        first
            .pointer("/items/0/gate_id")
            .and_then(Value::as_str)
            .map(str::to_owned),
        Some(fixture.gate.clone()),
        "oldest first: {first}"
    );
    let cursor = first
        .get("next_cursor")
        .and_then(Value::as_str)
        .expect("a full page hands back where it ended")
        .to_owned();

    let resumed = read(
        router,
        &fixture.token,
        collection,
        &format!("?limit=1&cursor={cursor}"),
    )
    .await;
    assert_eq!(
        resumed.pointer("/items/0/gate_id").and_then(Value::as_str),
        Some(second),
        "the second page resumes strictly after the first: {resumed}"
    );

    // The last page is short, so it ends the walk rather than pointing at rows
    // the store already said were not there.
    let last = read(router, &fixture.token, collection, "?limit=50").await;
    assert!(last.get("next_cursor").is_none_or(Value::is_null), "{last}");
}

/// A parameter this endpoint cannot read is refused, never ignored.
async fn assert_unreadable_parameters_refuse(
    router: &axum::Router,
    fixture: &Fixture,
    collection: &str,
) {
    for unreadable in [
        "?limit=0",
        "?limit=201",
        "?cursor=nonsense",
        "?status=elsewhere",
    ] {
        let refused = send(
            router,
            Method::GET,
            &format!("{collection}{unreadable}"),
            Some(&fixture.token),
            "",
        )
        .await;
        assert_eq!(
            refused.status(),
            StatusCode::BAD_REQUEST,
            "{unreadable} was served rather than refused"
        );
    }
}

/// One listing read, as JSON.
async fn read(router: &axum::Router, token: &str, collection: &str, query: &str) -> Value {
    let page = send(
        router,
        Method::GET,
        &format!("{collection}{query}"),
        Some(token),
        "",
    )
    .await;
    assert_eq!(page.status(), StatusCode::OK, "{query}");
    json_body(page).await
}
