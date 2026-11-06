//! Resume stays in the native admission queue and refuses foreign children.

use afd_auth::scope::{Scope, ScopeSet};
use afd_db::test_util::TestDatabase;
use http::{Method, StatusCode, header};
use serde_json::{Value, json};

use super::super::fixture::{SUBJECT, Tasks};
use super::{concurrent_control, patch, preserved, queued, read_task, resource_tag};
use crate::harness::{Fleet, json_body, send_with_headers};

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn task_resume_queues_one_continuation_and_refuses_foreign_children() {
    let lane = TestDatabase::shared();
    let fixture = Tasks::create(&lane).await;
    let router = Fleet::live(
        fixture.base.database.clone(),
        SUBJECT,
        ScopeSet::from_scopes(&[Scope::FleetRead, Scope::FleetWrite]),
    )
    .with_owned_workspace(fixture.base.workspace.clone())
    .router();
    let paused = pause(&router, &fixture).await;
    let ready_tag = resume(&router, &fixture, paused).await;
    assert_eq!(
        queued(&fixture).await,
        1,
        "retries and same-state requests cannot queue another lease"
    );
    foreign_control(&router, &fixture, &ready_tag).await;
    fixture.base.cleanup().await;
}

async fn pause(router: &axum::Router, fixture: &Tasks<'_>) -> (Value, String) {
    let (_, first_tag) = read_task(router, fixture, 2).await;
    let response = patch(
        router,
        fixture,
        2,
        json!({"state":"paused","expected_generation":1}),
        &first_tag,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let paused_tag = resource_tag(&response);
    (json_body(response).await, paused_tag)
}

async fn resume(
    router: &axum::Router,
    fixture: &Tasks<'_>,
    (paused, paused_tag): (Value, String),
) -> String {
    let body = json!({"state":"ready","expected_generation":2});
    let (ready, ready_tag) = concurrent_control(
        router,
        fixture,
        &fixture.item(2),
        &body.to_string(),
        &paused_tag,
    )
    .await;
    preserved(&paused, &ready);
    assert_eq!(ready.pointer("/context/state"), Some(&json!("ready")));
    assert_eq!(ready.pointer("/context/generation"), Some(&json!(3)));
    let response = patch(router, fixture, 2, body, &paused_tag).await;
    assert_eq!(response.status(), StatusCode::PRECONDITION_FAILED);
    let response = patch(
        router,
        fixture,
        2,
        json!({"state":"ready","expected_generation":3}),
        &ready_tag,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(json_body(response).await, ready);
    ready_tag
}

async fn foreign_control(router: &axum::Router, fixture: &Tasks<'_>, tag: &str) {
    let path = format!(
        "/v1/workspaces/{}/fleets/{}/tasks/{}",
        fixture.base.workspace,
        afd_db::test_util::mint_id(),
        fixture.ids[2]
    );
    let body = json!({"state":"cancelled","expected_generation":3}).to_string();
    let response = send_with_headers(
        router,
        Method::PATCH,
        &path,
        Some(&fixture.base.token),
        &body,
        &[(header::IF_MATCH, tag)],
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let foreign = fixture.base.other_workspace().await;
    let router = Fleet::live(
        fixture.base.database.clone(),
        SUBJECT,
        ScopeSet::from_scopes(&[Scope::FleetWrite]),
    )
    .with_owned_workspace(foreign.clone())
    .router();
    let path = format!(
        "/v1/workspaces/{foreign}/fleets/{}/tasks/{}",
        fixture.base.fleet, fixture.ids[2]
    );
    let response = send_with_headers(
        &router,
        Method::PATCH,
        &path,
        Some(&fixture.base.token),
        &body,
        &[(header::IF_MATCH, tag)],
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn task_resume_retains_exhausted_allowance_and_queues_nothing() {
    let lane = TestDatabase::shared();
    let fixture = Tasks::create(&lane).await;
    let router = Fleet::live(
        fixture.base.database.clone(),
        SUBJECT,
        ScopeSet::from_scopes(&[Scope::FleetRead, Scope::FleetWrite]),
    )
    .with_owned_workspace(fixture.base.workspace.clone())
    .router();
    let (_, paused_tag) = pause(&router, &fixture).await;
    let mut connection = fixture
        .base
        .database
        .acquire()
        .await
        .expect("exhaustion connection");
    sqlx::query("UPDATE fleet.tasks SET used_leases=max_leases WHERE id=$1::uuid")
        .bind(&fixture.ids[2])
        .execute(&mut *connection)
        .await
        .expect("spent admitted attempts");
    drop(connection);
    let (before, _) = read_task(&router, &fixture, 2).await;
    let response = patch(
        &router,
        &fixture,
        2,
        json!({"state":"ready","expected_generation":2}),
        &paused_tag,
    )
    .await;
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(crate::harness::code_of(response).await, "UZ-DT-005");
    let (after, _) = read_task(&router, &fixture, 2).await;
    assert_eq!(after, before);
    assert_eq!(queued(&fixture).await, 0, "refusal cannot create new work");
    fixture.base.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn task_resume_policy_refusal_rolls_back_with_the_original_state() {
    let lane = TestDatabase::shared();
    let fixture = Tasks::create(&lane).await;
    let router = Fleet::live(
        fixture.base.database.clone(),
        SUBJECT,
        ScopeSet::from_scopes(&[Scope::FleetRead, Scope::FleetWrite]),
    )
    .with_owned_workspace(fixture.base.workspace.clone())
    .router();
    let (before, paused_tag) = pause(&router, &fixture).await;
    let mut connection = fixture
        .base
        .database
        .acquire()
        .await
        .expect("setup connection");
    sqlx::query("UPDATE fleet.fleets SET head_revision_id=NULL WHERE id=$1::uuid")
        .bind(&fixture.base.fleet)
        .execute(&mut *connection)
        .await
        .expect("setup unavailable for new admissions");
    drop(connection);
    let response = patch(
        &router,
        &fixture,
        2,
        json!({"state":"ready","expected_generation":2}),
        &paused_tag,
    )
    .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = json_body(response).await;
    assert_eq!(body.get("error_code"), Some(&json!("UZ-AUTHOR-009")));
    assert_eq!(body.get("current_state"), Some(&json!("paused")));
    let (after, tag) = read_task(&router, &fixture, 2).await;
    assert_eq!(
        after, before,
        "failed staging rolls back the task transition"
    );
    assert_eq!(tag, paused_tag);
    assert_eq!(queued(&fixture).await, 0);
    fixture.base.cleanup().await;
}
