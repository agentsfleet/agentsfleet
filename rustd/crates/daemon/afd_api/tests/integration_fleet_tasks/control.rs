//! Public conditional controls fence concurrent writes without resetting work.
#![expect(clippy::expect_used, reason = "test preconditions fail loudly")]

mod adoption;
mod resume;

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use afd_auth::scope::{Scope, ScopeSet};
use afd_db::test_util::TestDatabase;
use http::{Method, StatusCode, header};
use serde_json::{Value, json};

use super::fixture::{SUBJECT, Tasks};
use crate::harness::{Fleet, json_body, send, send_with_headers};

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn task_controls_enforce_resource_tags_generation_and_terminal_fences() {
    let lane = TestDatabase::shared();
    let fixture = Tasks::create(&lane).await;
    let router = Fleet::live(
        fixture.base.database.clone(),
        SUBJECT,
        ScopeSet::from_scopes(&[Scope::FleetRead, Scope::FleetWrite]),
    )
    .with_owned_workspace(fixture.base.workspace.clone())
    .router();
    let path = fixture.item(0);
    let (before, tag) = read_task(&router, &fixture, 0).await;
    let pause = json!({"state":"paused", "expected_generation":1}).to_string();
    refusals(&router, &fixture, &path, &pause, &tag).await;
    let (paused, paused_tag) = concurrent_control(&router, &fixture, &path, &pause, &tag).await;
    preserved(&before, &paused);
    assert_eq!(paused.pointer("/context/state"), Some(&json!("paused")));
    assert_eq!(paused.pointer("/context/generation"), Some(&json!(2)));
    assert_eq!(paused.get("resource_version"), Some(&json!(2)));
    cancel(&router, &fixture, &paused_tag, &before, &paused).await;
    assert_eq!(queued(&fixture).await, 0, "stop controls cannot queue work");
    fixture.base.cleanup().await;
}

async fn cancel(
    router: &axum::Router,
    fixture: &Tasks<'_>,
    tag: &str,
    before: &Value,
    paused: &Value,
) {
    let response = patch(
        router,
        fixture,
        0,
        json!({"state":"paused","expected_generation":2}),
        tag,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        json_body(response).await,
        *paused,
        "same-state controls preserve the entire resource"
    );
    let (cancelled, cancelled_tag) = concurrent_control(
        router,
        fixture,
        &fixture.item(0),
        &json!({"state":"cancelled","expected_generation":2}).to_string(),
        tag,
    )
    .await;
    preserved(before, &cancelled);
    assert_eq!(
        cancelled.pointer("/context/state"),
        Some(&json!("cancelled"))
    );
    let response = patch(
        router,
        fixture,
        0,
        json!({"state":"paused","expected_generation":3}),
        &cancelled_tag,
    )
    .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(
        json_body(response).await.get("current_state"),
        Some(&json!("cancelled"))
    );
    let (after, _) = read_task(router, fixture, 0).await;
    assert_eq!(after, cancelled);
}

async fn refusals(router: &axum::Router, fixture: &Tasks<'_>, path: &str, pause: &str, tag: &str) {
    let missing = send(
        router,
        Method::PATCH,
        path,
        Some(&fixture.base.token),
        pause,
    )
    .await;
    assert_eq!(missing.status(), StatusCode::PRECONDITION_REQUIRED);
    assert_eq!(crate::harness::code_of(missing).await, "UZ-AUTHOR-004");
    for invalid in ["*", "W/\"weak\"", "\"stale\"", "\"a\", \"b\""] {
        let response = send_with_headers(
            router,
            Method::PATCH,
            path,
            Some(&fixture.base.token),
            pause,
            &[(header::IF_MATCH, invalid)],
        )
        .await;
        assert_eq!(response.status(), StatusCode::PRECONDITION_FAILED);
        let body = json_body(response).await;
        assert_eq!(body.get("etag"), Some(&json!(tag)));
        assert_eq!(body.get("error_code"), Some(&json!("UZ-DT-013")));
    }
    let wrong_generation = json!({"state":"paused", "expected_generation":2}).to_string();
    let response = send_with_headers(
        router,
        Method::PATCH,
        path,
        Some(&fixture.base.token),
        &wrong_generation,
        &[(header::IF_MATCH, tag)],
    )
    .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(
        json_body(response).await.get("current_state"),
        Some(&json!("running"))
    );
}

async fn concurrent_control(
    router: &axum::Router,
    fixture: &Tasks<'_>,
    path: &str,
    body: &str,
    tag: &str,
) -> (Value, String) {
    let barrier = Arc::new(tokio::sync::Barrier::new(100));
    let counts = Arc::new((AtomicUsize::new(0), AtomicUsize::new(0)));
    let mut pending = spawn_controls(router, fixture, path, body, tag, &barrier, &counts);
    let mut winner = None;
    let mut stale = 0;
    while let Some(response) = pending.join_next().await {
        let response = response.expect("caller completes");
        match response.status() {
            StatusCode::OK => {
                assert!(
                    winner.is_none(),
                    "one task transition wins the original resource tag"
                );
                let tag = resource_tag(&response);
                winner = Some((json_body(response).await, tag));
            }
            status => {
                assert_eq!(status, StatusCode::PRECONDITION_FAILED);
                assert_eq!(crate::harness::code_of(response).await, "UZ-DT-013");
                stale += 1;
            }
        }
    }
    assert_eq!(stale, 99);
    assert!(
        counts.1.load(Ordering::SeqCst) > 1,
        "callers overlap at the served route"
    );
    winner.expect("one winning control")
}

fn spawn_controls(
    router: &axum::Router,
    fixture: &Tasks<'_>,
    path: &str,
    body: &str,
    tag: &str,
    barrier: &Arc<tokio::sync::Barrier>,
    counts: &Arc<(AtomicUsize, AtomicUsize)>,
) -> tokio::task::JoinSet<axum::response::Response> {
    let mut pending = tokio::task::JoinSet::new();
    for _ in 0..100 {
        let counts = Arc::clone(counts);
        let (router, token, path, body, tag, barrier) = (
            router.clone(),
            fixture.base.token.clone(),
            path.to_owned(),
            body.to_owned(),
            tag.to_owned(),
            Arc::clone(barrier),
        );
        pending.spawn(async move {
            barrier.wait().await;
            let active = counts.0.fetch_add(1, Ordering::SeqCst) + 1;
            counts.1.fetch_max(active, Ordering::SeqCst);
            let response = send_with_headers(
                &router,
                Method::PATCH,
                &path,
                Some(&token),
                &body,
                &[(header::IF_MATCH, &tag)],
            )
            .await;
            counts.0.fetch_sub(1, Ordering::SeqCst);
            response
        });
    }
    pending
}

pub(super) fn preserved(before: &Value, after: &Value) {
    for field in [
        "task_id",
        "accepted_revision_id",
        "objective",
        "goal_digest",
        "questions",
        "covered_input_sequence",
        "effect_cursor",
        "confirmed_effect_watermark",
        "checkpoint_id",
        "expires_at",
        "limits",
        "usage",
    ] {
        assert_eq!(
            before.pointer(&format!("/context/{field}")),
            after.pointer(&format!("/context/{field}")),
            "control preserves {field}"
        );
    }
}

pub(super) async fn patch(
    router: &axum::Router,
    fixture: &Tasks<'_>,
    index: usize,
    body: Value,
    tag: &str,
) -> axum::response::Response {
    send_with_headers(
        router,
        Method::PATCH,
        &fixture.item(index),
        Some(&fixture.base.token),
        &body.to_string(),
        &[(header::IF_MATCH, tag)],
    )
    .await
}

pub(super) fn resource_tag(response: &axum::response::Response) -> String {
    response
        .headers()
        .get(header::ETAG)
        .expect("native resource tag")
        .to_str()
        .expect("ASCII tag")
        .to_owned()
}

pub(super) async fn queued(fixture: &Tasks<'_>) -> i64 {
    let mut connection = fixture
        .base
        .database
        .acquire()
        .await
        .expect("continuation read");
    sqlx::query_scalar("SELECT count(*) FROM fleet.admissions WHERE fleet_id=$1")
        .bind(&fixture.base.fleet)
        .fetch_one(&mut *connection)
        .await
        .expect("native work queue count")
}

pub(super) async fn read_task(
    router: &axum::Router,
    fixture: &Tasks<'_>,
    index: usize,
) -> (Value, String) {
    let response = send(
        router,
        Method::GET,
        &fixture.item(index),
        Some(&fixture.base.token),
        "",
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let tag = resource_tag(&response);
    (json_body(response).await, tag)
}
