//! A question takes an answer and only a question does; a repeated answer
//! stands, and an archived fleet's gate takes none. Live Postgres and NATS.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use crate::harness;
use crate::integration_workspace_approvals_fixture::Fixture;

use af_core::error_code;
use afd_auth::scope::{Scope, ScopeSet};
use http::{Method, StatusCode};
use serde_json::Value;

use self::harness::{Fleet, json_body, send};

/// This file's own signed-in person; the approval files run concurrently.
const SUBJECT: &str = "user_live_approval_answers";

async fn decide(
    router: &axum::Router,
    fixture: &Fixture,
    gate: &str,
    verb: &str,
    body: &str,
) -> (StatusCode, Value) {
    let route = format!(
        "/v1/workspaces/{}/approvals/{gate}/{verb}",
        fixture.workspace.as_str()
    );
    let response = send(router, Method::POST, &route, Some(&fixture.token), body).await;
    (response.status(), json_body(response).await)
}

fn code(body: &Value) -> Option<&str> {
    body.get("error_code").and_then(Value::as_str)
}

#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_question_takes_an_answer_that_stands_and_an_archived_fleet_takes_none() {
    let fixture = Fixture::create_as(SUBJECT).await;
    fixture.seed().await;
    let question = fixture
        .seed_gate(afd_api_wire::approval::GATE_TYPE_QUESTION, 3)
        .await;
    let router = Fleet::live(
        fixture.database.clone(),
        SUBJECT,
        ScopeSet::from_scopes(&Scope::ALL),
    )
    .with_owned_workspace(fixture.workspace.clone())
    .with_approval_queue(fixture.database.clone(), harness::bus().await)
    .router();
    let tool = fixture.gate.as_str().to_owned();

    // The decision has to match the gate's kind; a denial fits either.
    for (gate, verb, body) in [
        (question.as_str(), "approve", ""),
        (tool.as_str(), "answer", r#"{"answer":"Yes"}"#),
    ] {
        let (status, refused) = decide(&router, &fixture, gate, verb, body).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{verb}: {refused}");
        assert_eq!(code(&refused), Some(error_code::INVALID_REQUEST.as_str()));
        assert_eq!(
            refused.get("detail").and_then(Value::as_str),
            Some("a question takes an answer, and only a question does")
        );
    }

    let answer = r#"{"answer":"Ship it after the release"}"#;
    let (status, first) = decide(&router, &fixture, &question, "answer", answer).await;
    assert_eq!(status, StatusCode::OK, "{first}");
    assert_eq!(
        first.get("outcome").and_then(Value::as_str),
        Some("answered")
    );
    let (status, repeated) = decide(&router, &fixture, &question, "answer", answer).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "the same answer again stands: {repeated}"
    );
    assert_eq!(repeated, first, "a repeat reads the standing answer back");
    let (status, changed) = decide(
        &router,
        &fixture,
        &question,
        "answer",
        r#"{"answer":"Hold it"}"#,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{changed}");
    assert_eq!(
        code(&changed),
        Some(error_code::APPROVAL_ALREADY_RESOLVED.as_str())
    );

    sqlx::query("UPDATE fleet.fleets SET status = $2 WHERE id = $1")
        .bind(&fixture.fleet)
        .bind(afd_fleet_lifecycle::FleetStatus::Archived.as_str())
        .execute(&mut *fixture.database.acquire().await.expect("an API connection"))
        .await
        .expect("the fleet archives");
    let (status, archived) = decide(&router, &fixture, &tool, "approve", "").await;
    assert_eq!(status, StatusCode::CONFLICT, "{archived}");
    assert_eq!(
        code(&archived),
        Some(error_code::AGENTSFLEET_ARCHIVED.as_str())
    );
    let pending: String =
        sqlx::query_scalar("SELECT status FROM fleet.approval_gates WHERE id = $1")
            .bind(&fixture.gate)
            .fetch_one(&mut *fixture.database.acquire().await.expect("an API connection"))
            .await
            .expect("the tool gate reads back");
    assert_eq!(
        pending, "pending",
        "an archived fleet's gate is left unanswered"
    );
    fixture.cleanup().await;
}
