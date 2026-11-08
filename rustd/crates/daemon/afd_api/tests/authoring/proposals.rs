//! Runner proposals cross the actual bearer, strict input and native-edit boundary.

use super::{attempt::Running, json_body, send};
use http::{Method, StatusCode};
use serde_json::json;

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn proposal_router_commits_one_version_and_checks_stop_before_replay() {
    let r = Running::create().await;
    let request = r.initialize().to_string();
    let first = send(&r.router, Method::POST, &r.path, Some(&r.token), &request).await;
    assert_eq!(first.status(), StatusCode::ACCEPTED);
    let first = json_body(first).await;
    assert_eq!(first.pointer("/candidate/revision"), Some(&json!("1")));
    let replay = send(&r.router, Method::POST, &r.path, Some(&r.token), &request).await;
    assert_eq!(replay.status(), StatusCode::ACCEPTED);
    let replay = json_body(replay).await;
    assert_eq!(replay.get("candidate"), first.get("candidate"));
    assert_eq!(replay.get("replayed"), Some(&json!(true)));
    let path = format!("{}/{}/cancellations", r.f.collection(), r.draft);
    let stopped = send(
        &r.router,
        Method::POST,
        &path,
        Some(&r.f.token),
        r#"{"through_sequence":"1"}"#,
    )
    .await;
    assert_eq!(stopped.status(), StatusCode::OK);
    let late = send(&r.router, Method::POST, &r.path, Some(&r.token), &request).await;
    assert_eq!(late.status(), StatusCode::CONFLICT);
    assert_eq!(
        json_body(late).await.get("error_code"),
        Some(&json!("UZ-AUTHOR-012"))
    );
    r.f.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn proposal_router_rejects_person_tokens_unknown_authority_and_bad_counters() {
    let r = Running::create().await;
    let request = r.initialize();
    let person = send(
        &r.router,
        Method::POST,
        &r.path,
        Some(&r.f.token),
        &request.to_string(),
    )
    .await;
    assert_eq!(person.status(), StatusCode::UNAUTHORIZED);
    let mut authority = request.clone();
    authority
        .as_object_mut()
        .expect("object")
        .insert("grants".into(), json!([]));
    let response = send(
        &r.router,
        Method::POST,
        &r.path,
        Some(&r.token),
        &authority.to_string(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let mut invalid = request;
    *invalid.get_mut("activity_sequence").expect("sequence") = json!("01");
    let response = send(
        &r.router,
        Method::POST,
        &r.path,
        Some(&r.token),
        &invalid.to_string(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let candidates: i64 =
        sqlx::query_scalar("SELECT count(*) FROM authoring.candidates WHERE draft_id=$1::uuid")
            .bind(&r.draft)
            .fetch_one(&mut *r.f.db.acquire().await.expect("observer"))
            .await
            .expect("owned candidates");
    assert_eq!(candidates, 0);
    r.f.cleanup().await;
}
