//! Approval inbox and decision HTTP lifecycle over live Postgres and Dragonfly.
#![cfg(feature = "test-util")]

use crate::harness;
use crate::integration_workspace_approvals_fixture::Fixture;

use afd_auth::scope::{Scope, ScopeSet};
use afd_core::error_code;
use http::{Method, StatusCode};
use serde_json::Value;

use self::harness::{Fleet, json_body, send};

pub(crate) const SUBJECT: &str = "user_live_approval_inbox";

/// The listing suite's own signed-in person.
///
/// A separate subject because `core.users` holds one row per OIDC subject and
/// these files run concurrently: two fixtures seeding one subject race, and the
/// loser reports a duplicate key rather than the behaviour under test.
pub(crate) const LISTING_SUBJECT: &str = "user_live_approval_listing";

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn approval_inbox_reads_and_resolves_a_live_gate() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let queue = harness::connect_redis().await;
    let router = Fleet::live(
        fixture.database.clone(),
        SUBJECT,
        ScopeSet::from_scopes(&Scope::ALL),
    )
    .with_owned_workspace(fixture.workspace.clone())
    .with_approval_queue(fixture.database.clone(), queue)
    .router();
    let collection = format!("/v1/workspaces/{}/approvals", fixture.workspace.as_str());
    let item = format!("{collection}/{}", fixture.gate);
    assert_approval_reads(&router, &fixture, &collection, &item).await;
    assert_approval_resolution(&router, &fixture.token, &item, &fixture.gate, &collection).await;
    fixture.cleanup().await;
}

/// The address the fixture seeds on its `core.users` row.
///
/// The fixture's SQL cannot take a constant — the statement is a `&str` whose other
/// braces rule out `format!`, and every `$n` slot is spoken for — so the pair is
/// pinned by [`the_fixture_seeds_the_address_the_assertion_expects`] instead.
const SEEDED_EMAIL: &str = "approval-live@example.test";

/// The fixture SQL and the assertion name one address. A rename that touches
/// only one of them fails here rather than in a confusing body assertion.
#[test]
fn the_fixture_seeds_the_address_the_assertion_expects() {
    assert!(
        include_str!("integration_workspace_approvals_fixture.rs")
            .contains(&format!("'{SEEDED_EMAIL}'")),
        "the fixture SQL no longer seeds {SEEDED_EMAIL}"
    );
}

async fn assert_approval_reads(
    router: &axum::Router,
    fixture: &Fixture,
    collection: &str,
    item: &str,
) {
    let listed = send(router, Method::GET, collection, Some(&fixture.token), "").await;
    assert_eq!(listed.status(), StatusCode::OK);
    let listed = json_body(listed).await;
    assert_eq!(
        listed.pointer("/items/0/gate_id").and_then(Value::as_str),
        Some(fixture.gate.as_str())
    );

    let detail = send(router, Method::GET, item, Some(&fixture.token), "").await;
    assert_eq!(detail.status(), StatusCode::OK);
    assert_eq!(
        json_body(detail)
            .await
            .get("action_id")
            .and_then(Value::as_str),
        Some(fixture.action.as_str())
    );
}

async fn assert_approval_resolution(
    router: &axum::Router,
    token: &str,
    item: &str,
    gate: &str,
    collection: &str,
) {
    let resolved = send(
        router,
        Method::POST,
        &format!("{item}/approve"),
        Some(token),
        r#"{"reason":"reviewed"}"#,
    )
    .await;
    let status = resolved.status();
    let resolved = json_body(resolved).await;
    assert_eq!(status, StatusCode::OK, "{resolved}");
    assert_eq!(
        resolved.get("outcome").and_then(Value::as_str),
        Some("approved")
    );
    assert_eq!(
        resolved.get("resolved_by").and_then(Value::as_str),
        Some(SUBJECT)
    );

    // The name reaches the WIRE, not just the row. The schema declares the key required;
    // nothing pinned that `summary()` maps the right source field, so `resolved_by_name:
    // &gate.resolved_by` would compile, serialize, and match the schema. This lane's
    // `core.users` row carries an address and no display name, so it also exercises the
    // fallback the deleted browser lookup had.
    let after = send(router, Method::GET, collection, Some(token), "").await;
    assert_eq!(after.status(), StatusCode::OK);
    let after = json_body(after).await;
    assert_eq!(
        after
            .pointer("/items/0/resolved_by_name")
            .and_then(Value::as_str),
        Some(SEEDED_EMAIL),
        "the captured name is the deployment's own user row, not the subject"
    );

    // The second answer is a 409 rather than a 200 reporting the first. Both
    // tell the caller the gate is resolved; only the conflict tells them it was
    // not resolved BY THEM, which is the difference between an audit trail and
    // a dashboard that credits the wrong person for a denial.
    let repeated = send(
        router,
        Method::POST,
        &format!("{item}/deny"),
        Some(token),
        "",
    )
    .await;
    let status = repeated.status();
    let refused = json_body(repeated).await;
    assert_eq!(status, StatusCode::CONFLICT, "{refused}");
    assert_eq!(
        refused.get("error_code").and_then(Value::as_str),
        Some(error_code::APPROVAL_ALREADY_RESOLVED.as_str())
    );
    assert_eq!(
        refused.get("current_state").and_then(Value::as_str),
        Some("approved"),
        "the conflict names the standing outcome, so the caller refetches \
         rather than retrying a decision that cannot change"
    );
    // The resolver rides the envelope as an extension and stays OUT of the
    // sentence: a subject is an entity value, and the detail rules keep those
    // out of `detail`. `approvals/resolve.zig` draws the same line.
    assert!(
        !refused
            .get("detail")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .contains(SUBJECT),
        "the resolver was interpolated into the refusal sentence: {refused}"
    );
    assert_eq!(
        refused.get("resolved_by").and_then(Value::as_str),
        Some(SUBJECT),
        "the dashboard renders who resolved it off the body: {refused}"
    );
    assert_eq!(
        refused.get("outcome").and_then(Value::as_str),
        Some("approved"),
        "and what the standing answer was: {refused}"
    );
    assert_eq!(
        refused.get("gate_id").and_then(Value::as_str),
        Some(gate),
        "the conflict names the gate it is about: {refused}"
    );
    assert!(
        refused.get("resolved_at").and_then(Value::as_i64).is_some(),
        "when it was answered: {refused}"
    );
}
