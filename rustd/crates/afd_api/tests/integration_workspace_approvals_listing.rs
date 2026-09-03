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

/// The instant the second and third gates share.
///
/// Two rows in one millisecond is the ordinary case — a run parks several tools
/// at once — and it is the case a cursor carrying only an instant gets wrong.
const SHARED_INSTANT: i64 = 2;

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
    // A third gate sharing the second's instant: a cursor carrying only the
    // instant would skip it, and a fixture whose rows all differ cannot tell
    // that apart from a cursor that resumes correctly.
    let third = fixture.seed_gate("tool", SHARED_INSTANT).await;
    let other_fleet = fixture.seed_second_fleet().await;
    let elsewhere = fixture.seed_gate_for(&other_fleet, "tool", 4).await;
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

    assert_filters_narrow(&router, &fixture, &collection, &second, &elsewhere).await;
    assert_the_page_resumes(
        &router,
        &fixture,
        &collection,
        &[fixture.gate.clone(), second.clone(), third],
    )
    .await;
    assert_unreadable_parameters_refuse(&router, &fixture, &collection).await;

    fixture.cleanup().await;
}

/// Each filter selects, and a status nothing holds selects nothing.
async fn assert_filters_narrow(
    router: &axum::Router,
    fixture: &Fixture,
    collection: &str,
    second: &str,
    elsewhere: &str,
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
    assert_eq!(ids(&all).len(), 4, "every pending gate is waiting: {all}");

    let by_kind = read(router, &fixture.token, collection, "?gate_kind=spend").await;
    assert_eq!(ids(&by_kind), vec![second.to_owned()], "{by_kind}");

    // Asserted by EXCLUSION, not by count: a filter that is ignored returns
    // the unfiltered page, and a count alone cannot tell the two apart.
    let by_fleet = read(
        router,
        &fixture.token,
        collection,
        &format!("?fleet_id={}", fixture.fleet),
    )
    .await;
    let narrowed = ids(&by_fleet);
    assert_eq!(narrowed.len(), 3, "{by_fleet}");
    assert!(
        !narrowed.iter().any(|gate| gate == elsewhere),
        "another fleet's gate survived the filter: {by_fleet}"
    );

    // Both gates are pending, so a resolved status selects none of them. An
    // ignored parameter would answer the pending page here and look correct.
    let resolved = read(router, &fixture.token, collection, "?status=approved").await;
    assert!(ids(&resolved).is_empty(), "{resolved}");
    assert!(
        resolved.get("next_cursor").is_none_or(Value::is_null),
        "an empty page points nowhere: {resolved}"
    );

    let explicit_pending = read(router, &fixture.token, collection, "?status=pending").await;
    assert_eq!(ids(&explicit_pending).len(), 4, "{explicit_pending}");

    // The killer's verdict is a state a row is really in, and this listing is
    // the only place it shows. A filter that could not name it left those
    // gates unreachable rather than merely unfiltered.
    let killed = read(router, &fixture.token, collection, "?status=auto_killed").await;
    assert!(
        ids(&killed).is_empty(),
        "nothing was auto-killed in this fixture: {killed}"
    );
}

/// A full page hands back a cursor, and the walk visits every row exactly once.
///
/// Asserted as a UNION over pages rather than as "page two differs from page
/// one": the endpoint promises `(created_at, id)` ordering precisely so gates
/// sharing a millisecond are not skipped, and two rows that merely differ
/// cannot show that.
async fn assert_the_page_resumes(
    router: &axum::Router,
    fixture: &Fixture,
    collection: &str,
    expected: &[String],
) {
    let mut seen: Vec<String> = Vec::new();
    let mut query = "?limit=1&fleet_id=".to_owned() + &fixture.fleet;
    for _page in 0..expected.len() {
        let page = read(router, &fixture.token, collection, &query).await;
        let gate = page
            .pointer("/items/0/gate_id")
            .and_then(Value::as_str)
            .expect("a page of one carries one gate")
            .to_owned();
        seen.push(gate);
        let cursor = page
            .get("next_cursor")
            .and_then(Value::as_str)
            .expect("a full page hands back where it ended")
            .to_owned();
        // Encoded the way a browser sends it: `URLSearchParams` escapes the
        // colon in the clear wire form, and reading it raw refuses every page
        // after the first.
        query = format!(
            "?limit=1&fleet_id={}&cursor={}",
            fixture.fleet,
            cursor.replace(':', "%3A")
        );
    }

    let mut unique = seen.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(
        unique.len(),
        seen.len(),
        "the walk returned a row twice: {seen:?}"
    );
    let mut wanted: Vec<String> = expected.to_vec();
    wanted.sort_unstable();
    assert_eq!(unique, wanted, "the walk skipped a row: {seen:?}");
    assert_eq!(
        seen.first().map(String::as_str),
        Some(fixture.gate.as_str()),
        "oldest first"
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
