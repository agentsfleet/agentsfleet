//! A purged fleet leaves a bounded, content-free Save receipt.

use super::{
    super::{json_body, mint, send_with_headers},
    fixture::Case,
};
use afd_core::error_code;
use http::{Method, StatusCode, header};

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn retrying_save_after_purge_returns_gone_without_allocating_another_fleet() {
    let case = Case::create().await;
    case.fund().await;
    let (_, request) = case.request().await;
    let key = mint();
    let saved = case.save(key.as_str(), &request).await;
    assert_eq!(saved.status(), StatusCode::CREATED);
    let item = saved.headers()[header::LOCATION]
        .to_str()
        .expect("fleet location")
        .to_owned();
    let tag = saved.headers()[header::ETAG]
        .to_str()
        .expect("fleet tag")
        .to_owned();
    let killed = send_with_headers(
        &case.router,
        Method::PATCH,
        &item,
        Some(&case.base.token),
        r#"{"status":"killed"}"#,
        &[(header::IF_MATCH, &tag)],
    )
    .await;
    assert_eq!(killed.status(), StatusCode::OK);
    let killed = json_body(killed).await;
    let current = killed["etag"].as_str().expect("killed tag");
    assert_delete_refusals(&case, &item, &tag, current).await;
    assert_eq!(
        remove(&case, &item, Some(current)).await.status(),
        StatusCode::NO_CONTENT
    );
    let replay = case.save(key.as_str(), &request).await;
    assert_eq!(replay.status(), StatusCode::GONE);
    assert_eq!(
        json_body(replay).await["error_code"],
        error_code::AUTHORING_GONE.as_str()
    );
    assert_retained_identity(&case, &key).await;
    case.cleanup().await;
}

async fn remove(case: &Case, item: &str, tag: Option<&str>) -> axum::response::Response {
    let headers: Vec<_> = tag.map(|tag| (header::IF_MATCH, tag)).into_iter().collect();
    send_with_headers(
        &case.router,
        Method::DELETE,
        item,
        Some(&case.base.token),
        "",
        &headers,
    )
    .await
}

async fn assert_delete_refusals(case: &Case, item: &str, stale: &str, current: &str) {
    let missing = remove(case, item, None).await;
    assert_eq!(missing.status(), StatusCode::PRECONDITION_REQUIRED);
    assert_eq!(
        json_body(missing).await["error_code"],
        error_code::AUTHORING_PRECONDITION.as_str()
    );
    let stale = remove(case, item, Some(stale)).await;
    assert_eq!(stale.status(), StatusCode::PRECONDITION_FAILED);
    assert_eq!(json_body(stale).await["etag"], current);
}

async fn assert_retained_identity(case: &Case, key: &afd_core::id::Uuid7) {
    let mut connection = case.base.db.acquire().await.expect("purge inspection");
    let remaining: i64 =
        sqlx::query_scalar("SELECT count(*) FROM core.fleets WHERE workspace_id=$1::uuid")
            .bind(case.base.workspace.as_str())
            .fetch_one(&mut *connection)
            .await
            .expect("fleet census");
    assert_eq!(remaining, 0);
    let receipt: (i64, i64) = sqlx::query_as("SELECT purged_at,expires_at FROM core.authoring_replay_receipts WHERE operation_id=$1::uuid")
        .bind(key.as_str()).fetch_one(&mut *connection).await.expect("retained receipt");
    assert_eq!(
        receipt.1 - receipt.0,
        afd_wire::authoring::REPLAY_TOMBSTONE_MS
    );
    drop(connection);
}

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_current_tag_does_not_authorize_purging_a_live_or_foreign_fleet() {
    let case = Case::create().await;
    case.fund().await;
    let (_, request) = case.request().await;
    let saved = case.save(mint().as_str(), &request).await;
    assert_eq!(saved.status(), StatusCode::CREATED);
    let item = saved.headers()[header::LOCATION]
        .to_str()
        .expect("fleet location")
        .to_owned();
    let tag = saved.headers()[header::ETAG]
        .to_str()
        .expect("fleet tag")
        .to_owned();
    let refusal = remove(&case, &item, Some(&tag)).await;
    assert_eq!(refusal.status(), StatusCode::CONFLICT);
    let refusal = json_body(refusal).await;
    assert_eq!(
        refusal["error_code"],
        error_code::AGENTSFLEET_ALREADY_TERMINAL.as_str()
    );
    assert_eq!(refusal["current_state"], "kill_required");
    let foreign = remove(
        &case,
        &format!("{}/{}", case.collection(), mint()),
        Some(&tag),
    )
    .await;
    assert_eq!(foreign.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        json_body(foreign).await["error_code"],
        error_code::AGENTSFLEET_NOT_FOUND.as_str()
    );
    let current =
        super::super::send(&case.router, Method::GET, &item, Some(&case.base.token), "").await;
    assert_eq!(current.status(), StatusCode::OK);
    assert_eq!(json_body(current).await["status"], "active");
    case.cleanup().await;
}
