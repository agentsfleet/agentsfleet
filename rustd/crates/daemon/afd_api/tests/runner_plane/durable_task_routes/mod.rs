//! The durable-task run verbs over live Postgres and NATS, through the
//! production router: a run binds a task from its person's message, reads and
//! acknowledges its inputs, asks the decision model whether the goal is met,
//! checkpoints its state and reads its publications.
//!
//! A binding leaves its run an ordinary run; a task's own runs are its
//! continuation's. A goal check, a checkpoint and a publication belong to a
//! task run, so once the binding is accepted the fixture run takes the task as
//! its continuation's run would (`DurableTasks::attach_run`).
#![expect(
    clippy::expect_used,
    reason = "integration fixture preconditions fail loudly"
)]

mod binding;
mod checkpoint_objects;
mod checkpoints;
mod decisions;
mod effects;
mod fixture;

use http::{Method, StatusCode};
use serde_json::{Value, json};

use crate::harness::{json_body, send};
use fixture::Fixture;

/// `resource` under the fixture run's durable-task verbs.
fn path(fixture: &Fixture, resource: &str) -> String {
    format!(
        "/v1/runners/me/runs/{}/durable_tasks/{resource}",
        fixture.run.id
    )
}

/// `route` with the run's fence as its query.
fn fenced(fixture: &Fixture, route: &str) -> String {
    format!("{route}?fencing_token={}", fixture.run.fence)
}

/// One request as the run's runner, answered `status`; a success carries
/// `no-store`.
async fn call(
    fixture: &Fixture,
    method: Method,
    route: &str,
    body: &Value,
    status: StatusCode,
) -> Value {
    let body = if body.is_null() {
        String::new()
    } else {
        body.to_string()
    };
    let response = send(
        &fixture.router,
        method,
        route,
        Some(&fixture.runner.token),
        &body,
    )
    .await;
    let actual = response.status();
    if actual.is_success() {
        assert_eq!(
            response
                .headers()
                .get(http::header::CACHE_CONTROL)
                .expect("a task answer names its caching"),
            "no-store"
        );
    }
    let value = json_body(response).await;
    assert_eq!(actual, status, "{route}: {value}");
    value
}

/// A `GET` of `route` as the run's runner, answered `status`.
async fn read(fixture: &Fixture, route: &str, status: StatusCode) -> Value {
    call(fixture, Method::GET, route, &Value::Null, status).await
}

/// A problem body carrying `code`, a request id and a detail.
fn problem(body: &Value, code: &str) {
    assert_eq!(body.pointer("/error_code"), Some(&json!(code)), "{body}");
    for field in ["/request_id", "/detail"] {
        assert!(
            body.pointer(field)
                .and_then(Value::as_str)
                .is_some_and(|text| !text.is_empty()),
            "{field}: {body}"
        );
    }
}

/// `body` refused with no credential, from another runner, and under a fence
/// one past the run's.
async fn foreign_and_stale(fixture: &Fixture, method: Method, route: &str, body: &Value) {
    for (token, status, code) in [
        (None, StatusCode::UNAUTHORIZED, "UZ-RUN-001"),
        // The run lookup hides a run that belongs to another runner.
        (
            Some(fixture.other.token.as_str()),
            StatusCode::NOT_FOUND,
            "UZ-RUN-006",
        ),
    ] {
        let response = send(
            &fixture.router,
            method.clone(),
            route,
            token,
            &body.to_string(),
        )
        .await;
        assert_eq!(response.status(), status, "{route}");
        problem(&json_body(response).await, code);
    }
    let mut stale = body.clone();
    *stale
        .get_mut("fencing_token")
        .expect("a fenced body names its fence") = json!(fixture.run.fence + 1);
    let refused = call(fixture, method, route, &stale, StatusCode::CONFLICT).await;
    problem(&refused, "UZ-RUN-005");
}

/// One field of `value` at `pointer`, which the answer must carry; a missing
/// one fails with the whole answer.
fn field<'v>(value: &'v Value, pointer: &str) -> &'v Value {
    let found = value.pointer(pointer);
    assert!(found.is_some(), "{pointer}: {value}");
    found.unwrap_or(&Value::Null)
}

/// The string `value` carries at `pointer`.
fn text<'v>(value: &'v Value, pointer: &str) -> &'v str {
    let found = field(value, pointer).as_str();
    assert!(found.is_some(), "{pointer} is text: {value}");
    found.unwrap_or_default()
}
