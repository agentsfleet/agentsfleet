//! A real lifecycle transition invalidates the previous strong resource tag.
#![expect(clippy::expect_used, reason = "test preconditions fail loudly")]

use afd_fleet::durable_task::{DurableTaskControl, DurableTaskLifecycle as _, DurableTaskScope};
use http::{Method, StatusCode, header};
use serde_json::{Value, json};

use super::fixture::DurableTasks;
use crate::harness::{json_body, send_with_headers};

pub(super) async fn change(
    router: &axum::Router,
    fixture: &DurableTasks<'_>,
    previous_tag: &str,
    previous: &Value,
) {
    let scope = DurableTaskScope {
        workspace_id: fixture.base.workspace.clone(),
        fleet_id: fixture.base.fleet.clone(),
    };
    let task = fixture.ids[1].clone();
    let changed = fixture
        .lifecycle()
        .control(&scope, &task, previous_tag, 1, DurableTaskControl::Pause)
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
