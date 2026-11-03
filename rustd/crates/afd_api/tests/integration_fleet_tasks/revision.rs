//! A real lifecycle transition invalidates the previous strong resource tag.
#![expect(clippy::expect_used, reason = "test preconditions fail loudly")]

use afd_core::id::Uuid7;
use afd_fleet::task::{TaskControl, TaskLifecycle as _, TaskScope};
use http::{Method, StatusCode, header};
use serde_json::{Value, json};

use super::fixture::Tasks;
use crate::harness::{json_body, send_with_headers};

/// Read fixtures append history directly; this does not prove the authoring Apply flow.
pub(super) async fn advance(fixture: &Tasks<'_>) {
    let mut value: Value = serde_json::from_str(afd_db::test_util::FIXTURE_SETUP_DEFINITION)
        .expect("native fixture definition");
    *value
        .get_mut("instructions_md")
        .expect("native instructions") = json!(
        "Use the revised instructions for new work; existing tasks retain their accepted goals."
    );
    let definition = afd_fleet_runtime::definition::Definition::parse(
        &serde_json::to_string(&value).expect("revised fixture document"),
    )
    .expect("revised definition remains valid");
    let digest = afd_fleet_runtime::definition::canonical_digest(&definition)
        .expect("native revised content identity");
    let mut connection = fixture
        .base
        .database
        .acquire()
        .await
        .expect("history connection");
    let revision: i64 = sqlx::query_scalar(include_str!("advance_revision.sql"))
        .bind(afd_db::test_util::mint_id())
        .bind(&fixture.base.fleet)
        .bind(serde_json::to_value(definition).expect("typed revised definition"))
        .bind(digest)
        .fetch_one(&mut *connection)
        .await
        .expect("new immutable history and fleet head commit together");
    assert_eq!(
        revision, 2,
        "the fleet head advanced before the pinned task read"
    );
}

pub(super) async fn change(
    router: &axum::Router,
    fixture: &Tasks<'_>,
    previous_tag: &str,
    previous: &Value,
) {
    let scope = TaskScope {
        workspace_id: fixture.base.workspace.clone(),
        fleet_id: Uuid7::parse(&fixture.base.fleet).expect("fleet identity"),
    };
    let task = Uuid7::parse(&fixture.ids[1]).expect("task identity");
    let changed = fixture
        .lifecycle()
        .control(&scope, &task, 1, TaskControl::Pause)
        .await
        .expect("native pause");
    assert_eq!(changed.resource_version, 2);
    let response = send_with_headers(
        router,
        Method::GET,
        &fixture.item(1),
        Some(&fixture.base.token),
        "",
        &[(header::IF_NONE_MATCH, previous_tag)],
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_ne!(
        response.headers().get(header::ETAG).expect("new tag"),
        previous_tag
    );
    let updated = json_body(response).await;
    assert_eq!(updated.pointer("/context/state"), Some(&json!("paused")));
    assert_eq!(
        updated.pointer("/context/questions"),
        previous.pointer("/context/questions")
    );
    assert_eq!(
        updated.pointer("/context/usage"),
        previous.pointer("/context/usage"),
        "control retains cumulative usage"
    );
}
