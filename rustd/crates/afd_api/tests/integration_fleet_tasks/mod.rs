//! Read real task state through served routes, scopes, paging and resource tags.
#![cfg(feature = "test-util")]
#![expect(clippy::expect_used, reason = "test preconditions fail loudly")]

mod deadline;
mod fixture;
mod revision;

use afd_auth::scope::{Scope, ScopeSet};
use afd_db::test_util::{TestDatabase, mint_id};
use http::{Method, StatusCode, header};
use serde_json::{Value, json};

use crate::harness::{Fleet, code_of, items, json_body, send, send_with_headers};
use fixture::{SUBJECT, Tasks};

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn task_reads_preserve_scope_paging_accepted_questions_and_resource_tags() {
    let lane = TestDatabase::shared();
    let fixture = Tasks::create(&lane).await;
    let router = Fleet::live(
        fixture.base.database.clone(),
        SUBJECT,
        ScopeSet::from_scopes(&[Scope::FleetRead]),
    )
    .with_owned_workspace(fixture.base.workspace.clone())
    .router();
    pages(&router, &fixture).await;
    detail(&router, &fixture).await;
    foreign(&router, &fixture).await;
    fixture.base.cleanup().await;
}

async fn pages(router: &axum::Router, fixture: &Tasks<'_>) {
    let path = format!("{}?limit=2", fixture.collection());
    let response = send(router, Method::GET, &path, Some(&fixture.base.token), "").await;
    assert_eq!(response.status(), StatusCode::OK);
    let first = json_body(response).await;
    assert_eq!(items(&first).len(), 2);
    assert_eq!(
        projected(&first),
        vec![
            (fixture.ids[0].as_str(), "running"),
            (fixture.ids[2].as_str(), "ready")
        ]
    );
    assert!(
        items(&first)
            .first()
            .expect("first task")
            .get("objective")
            .is_none(),
        "compact reads exclude private objective bodies"
    );
    let cursor = first
        .get("next_cursor")
        .expect("page cursor field")
        .as_str()
        .expect("a third unfinished task exists");
    let cursor = cursor.replace(':', "%3A");
    let next = send(
        router,
        Method::GET,
        &format!("{path}&starting_after={cursor}"),
        Some(&fixture.base.token),
        "",
    )
    .await;
    assert_eq!(next.status(), StatusCode::OK);
    let next = json_body(next).await;
    assert_eq!(items(&next).len(), 1);
    assert_eq!(projected(&next), vec![(fixture.ids[1].as_str(), "waiting")]);
    assert_eq!(next.get("next_cursor"), Some(&json!(null)));
    let empty = fixture.base.empty_fleet().await;
    let path = format!(
        "/v1/workspaces/{}/fleets/{empty}/tasks",
        fixture.base.workspace
    );
    let response = send(router, Method::GET, &path, Some(&fixture.base.token), "").await;
    assert_eq!(response.status(), StatusCode::OK);
    let empty = json_body(response).await;
    assert!(
        items(&empty).is_empty(),
        "an owned empty fleet is confirmed empty"
    );
    assert_eq!(empty.get("next_cursor"), Some(&json!(null)));
}

async fn detail(router: &axum::Router, fixture: &Tasks<'_>) {
    let path = fixture.item(1);
    let response = send(router, Method::GET, &path, Some(&fixture.base.token), "").await;
    assert_eq!(response.status(), StatusCode::OK);
    let tag = response
        .headers()
        .get(header::ETAG)
        .expect("strong tag")
        .to_str()
        .expect("ASCII tag")
        .to_owned();
    let body = json_body(response).await;
    assert_eq!(body.get("title"), Some(&json!("PR #124 repair")));
    assert_eq!(body.get("setup_revision"), Some(&json!(1)));
    assert_eq!(body.pointer("/context/state"), Some(&json!("waiting")));
    assert_eq!(
        body.pointer("/context/task_id"),
        Some(&json!(fixture.ids[1]))
    );
    assert_eq!(
        body.pointer("/context/questions/0/question/instructions"),
        Some(&json!("Does the patch fix the accepted defect?"))
    );
    assert_eq!(
        body.pointer("/context/usage/model_nanos"),
        Some(&json!("12000000"))
    );
    assert_eq!(
        body.pointer("/context/usage/reserved_model_nanos"),
        Some(&json!("20000000"))
    );
    assert_eq!(body.pointer("/context/limits/leases"), Some(&json!(12)));
    revision::advance(fixture).await;
    let pinned = send(router, Method::GET, &path, Some(&fixture.base.token), "").await;
    assert_eq!(pinned.status(), StatusCode::OK);
    assert_eq!(
        json_body(pinned).await,
        body,
        "new fleet setup does not replace the task's accepted revision or criteria"
    );
    let same = send_with_headers(
        router,
        Method::GET,
        &path,
        Some(&fixture.base.token),
        "",
        &[(header::IF_NONE_MATCH, &tag)],
    )
    .await;
    assert_eq!(same.status(), StatusCode::NOT_MODIFIED);
    assert_eq!(
        same.headers().get(header::ETAG).expect("unchanged tag"),
        tag.as_str()
    );
    let bytes = axum::body::to_bytes(same.into_body(), 1024)
        .await
        .expect("empty 304 body");
    assert!(bytes.is_empty());
    revision::change(router, fixture, &tag, &body).await;
    let done = send(
        router,
        Method::GET,
        &fixture.item(3),
        Some(&fixture.base.token),
        "",
    )
    .await;
    assert_eq!(done.status(), StatusCode::OK);
    assert_eq!(
        json_body(done).await.pointer("/context/state"),
        Some(&json!("done")),
        "terminal work remains directly readable"
    );
}

async fn foreign(router: &axum::Router, fixture: &Tasks<'_>) {
    let path = format!(
        "/v1/workspaces/{}/fleets/{}/tasks/{}",
        fixture.base.workspace,
        mint_id(),
        fixture.ids[0]
    );
    let response = send(router, Method::GET, &path, Some(&fixture.base.token), "").await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(code_of(response).await, "UZ-DT-001");
    let foreign_workspace = fixture.base.other_workspace().await;
    let foreign_scope = Fleet::live(
        fixture.base.database.clone(),
        SUBJECT,
        ScopeSet::from_scopes(&[Scope::FleetRead]),
    )
    .with_owned_workspace(foreign_workspace.clone())
    .router();
    let response = send(
        &foreign_scope,
        Method::GET,
        &format!(
            "/v1/workspaces/{foreign_workspace}/fleets/{}/tasks/{}",
            fixture.base.fleet, fixture.ids[0]
        ),
        Some(&fixture.base.token),
        "",
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(code_of(response).await, "UZ-DT-001");
    let collection = format!(
        "/v1/workspaces/{foreign_workspace}/fleets/{}/tasks",
        fixture.base.fleet
    );
    let response = send(
        &foreign_scope,
        Method::GET,
        &collection,
        Some(&fixture.base.token),
        "",
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(code_of(response).await, "UZ-DT-001");
}

fn projected(document: &Value) -> Vec<(&str, &str)> {
    items(document)
        .iter()
        .map(|row| {
            (
                row.get("id")
                    .and_then(Value::as_str)
                    .expect("task identity"),
                row.get("status")
                    .and_then(Value::as_str)
                    .expect("native task state"),
            )
        })
        .collect()
}
