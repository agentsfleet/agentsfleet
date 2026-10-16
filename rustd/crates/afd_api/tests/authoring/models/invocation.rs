//! A model proposal has no candidate until the fixed activity handler accepts it.

use super::super::{json_body, send};
use super::fixture::Model;
use axum::response::Response;
use http::{Method, StatusCode};
use serde_json::{Value, json};
use std::sync::atomic::Ordering;

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn authenticated_model_result_replays_and_only_activity_commits_its_proposal() {
    let model = Model::create().await;
    let response = invoke(&model).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("cache-control")
            .expect("private cache policy"),
        "no-store"
    );
    let result = json_body(response).await;
    assert_eq!(result.get("replayed"), Some(&Value::Bool(false)));
    let calls = result
        .get("proposal_calls")
        .and_then(Value::as_array)
        .expect("fixed proposal calls");
    assert_eq!(calls.len(), 1);
    accept_proposal(&model, calls).await;
    let replayed = invoke(&model).await;
    assert_eq!(replayed.status(), StatusCode::OK);
    assert_eq!(
        json_body(replayed).await.get("replayed"),
        Some(&Value::Bool(true))
    );
    assert_eq!(model.provider.effects.load(Ordering::SeqCst), 1);
    assert_eq!(model.receipts().await, 1);
    assert_eq!(candidate_count(&model).await, 1);
    refuse_after_stop(&model).await;
    model.running.f.cleanup().await;
}

/// One model call through the machine route, with the fixture's request.
async fn invoke(model: &Model) -> Response {
    let r = &model.running;
    send(
        &r.router,
        Method::POST,
        &model.path,
        Some(&r.token),
        &model.request.to_string(),
    )
    .await
}

/// How many candidates the draft holds.
async fn candidate_count(model: &Model) -> i64 {
    let r = &model.running;
    sqlx::query_scalar("SELECT count(*) FROM core.authoring_candidates WHERE draft_id=$1::uuid")
        .bind(&r.draft)
        .fetch_one(&mut *r.f.db.acquire().await.expect("candidate read"))
        .await
        .expect("candidate count")
}

async fn accept_proposal(model: &Model, calls: &[Value]) {
    let r = &model.running;
    let proposal = calls
        .first()
        .and_then(|call| call.get("proposal"))
        .expect("native proposal");
    let activity = json!({"fence":"1","deletion_generation":"0","sequence":"1","activity_sequence":"1",
        "event":{"kind":"proposal","proposal":proposal}});
    let accepted = send(
        &r.router,
        Method::POST,
        &r.path,
        Some(&r.token),
        &activity.to_string(),
    )
    .await;
    assert_eq!(accepted.status(), StatusCode::ACCEPTED);
}

async fn refuse_after_stop(model: &Model) {
    let r = &model.running;
    let stopped = send(
        &r.router,
        Method::POST,
        &format!("{}/{}/cancellations", r.f.collection(), r.draft),
        Some(&r.f.token),
        r#"{"through_sequence":"1"}"#,
    )
    .await;
    assert_eq!(stopped.status(), StatusCode::OK);
    let refused = invoke(model).await;
    assert_eq!(refused.status(), StatusCode::CONFLICT);
    assert_eq!(
        json_body(refused)
            .await
            .get("error_code")
            .and_then(Value::as_str),
        Some("UZ-AUTHOR-012")
    );
}
