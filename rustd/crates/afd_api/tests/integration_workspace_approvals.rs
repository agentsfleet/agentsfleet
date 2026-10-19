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
/// A separate subject because `identity.users` holds one row per OIDC subject and
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

/// A gate and a fleet this workspace does not hold are each answered as
/// absent by the live stores: a decision on an unknown gate is the approval's
/// own 404, and revoking a grant under an unknown fleet is the fleet's, so a
/// caller learns which half of the path named nothing.
#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn an_unknown_gate_and_an_unknown_fleet_are_each_absent() {
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
    let workspace = fixture.workspace.as_str();
    let nowhere = afd_db::test_util::mint_id();

    let decided = send(
        &router,
        Method::POST,
        &format!("/v1/workspaces/{workspace}/approvals/{nowhere}/approve"),
        Some(&fixture.token),
        r#"{"reason":"reviewed"}"#,
    )
    .await;
    let status = decided.status();
    let decided = json_body(decided).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{decided}");
    assert_eq!(
        decided.get("error_code").and_then(Value::as_str),
        Some(error_code::APPROVAL_NOT_FOUND.as_str())
    );

    let grant = afd_db::test_util::mint_id();
    let revoked = send(
        &router,
        Method::DELETE,
        &format!("/v1/workspaces/{workspace}/fleets/{nowhere}/integration-grants/{grant}"),
        Some(&fixture.token),
        "",
    )
    .await;
    let status = revoked.status();
    let revoked = json_body(revoked).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{revoked}");
    assert_eq!(
        revoked.get("error_code").and_then(Value::as_str),
        Some(error_code::AGENTSFLEET_NOT_FOUND.as_str())
    );
    fixture.cleanup().await;
}

/// The vanishing-gate test's own signed-in person, for the reason
/// [`LISTING_SUBJECT`] gives.
const VANISH_SUBJECT: &str = "user_live_approval_vanish";

/// The action id the concurrent edit moves the gate to.
const MOVED_ACTION: &str = "moved-away";

/// A gate that leaves its action between the handler's read and its decision
/// is answered as absent, not as resolved by nobody.
///
/// An open transaction moves the gate off its action id and holds the row; the
/// decision's update queues behind it. Once the edit commits, the update
/// re-checks a row that no longer matches and the follow-up read finds no gate
/// under that action. The wait polls `pg_stat_activity`, a catalog view no
/// test alters, never the gate table.
#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_gate_that_vanishes_mid_decision_is_absent() {
    let fixture = Fixture::create_as(VANISH_SUBJECT).await;
    fixture.seed().await;
    let queue = harness::connect_redis().await;
    let router = Fleet::live(
        fixture.database.clone(),
        VANISH_SUBJECT,
        ScopeSet::from_scopes(&Scope::ALL),
    )
    .with_owned_workspace(fixture.workspace.clone())
    .with_approval_queue(fixture.database.clone(), queue)
    .router();

    let mut editor = fixture
        .database
        .acquire()
        .await
        .expect("an editing connection");
    let mut edit = sqlx::Acquire::begin(&mut *editor)
        .await
        .expect("the edit opens");
    let editor_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *edit)
        .await
        .expect("the editor names its backend");
    sqlx::query("UPDATE fleet.approval_gates SET action_id = $2 WHERE id = $1::uuid")
        .bind(&fixture.gate)
        .bind(MOVED_ACTION)
        .execute(&mut *edit)
        .await
        .expect("the gate moves inside the open edit");

    let path = format!(
        "/v1/workspaces/{}/approvals/{}/approve",
        fixture.workspace.as_str(),
        fixture.gate
    );
    let token = fixture.token.clone();
    let decision = tokio::spawn({
        let router = router.clone();
        async move { send(&router, Method::POST, &path, Some(&token), "").await }
    });

    let mut watcher = fixture
        .database
        .acquire()
        .await
        .expect("a watching connection");
    let blocked = tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            let waiting: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM pg_stat_activity \
                 WHERE $1 = ANY(pg_blocking_pids(pid))",
            )
            .bind(editor_pid)
            .fetch_one(&mut *watcher)
            .await
            .expect("the activity view answers");
            if waiting > 0 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await;
    edit.commit().await.expect("the edit commits");
    blocked.expect("the decision queued behind the edit");

    let decided = decision.await.expect("the decision task joins");
    let status = decided.status();
    let decided = json_body(decided).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{decided}");
    assert_eq!(
        decided.get("error_code").and_then(Value::as_str),
        Some(error_code::APPROVAL_NOT_FOUND.as_str())
    );
    drop(watcher);
    drop(editor);
    fixture.cleanup().await;
}

/// The address the fixture seeds on its `identity.users` row.
///
/// The fixture's SQL cannot take a constant — the statement is a `&str` whose other
/// braces rule out `format!`, and every `$n` slot is spoken for — so a fixture
/// that seeds another address fails the read's assertion on this one.
const SEEDED_EMAIL: &str = "approval-live@example.test";

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
    // `identity.users` row carries an address and no display name, so it also exercises the
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
    // out of `detail`.
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
        refused
            .get("resolved_at")
            .and_then(Value::as_str)
            .is_some_and(|at| at.parse::<jiff::Timestamp>().is_ok()),
        "when it was answered: {refused}"
    );
}
