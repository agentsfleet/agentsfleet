//! A purged fleet leaves a bounded, content-free Save receipt.

use super::{
    super::{json_body, mint, send_with_headers},
    fixture::Case,
};
use af_core::error_code;
use http::{Method, StatusCode, header};

#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
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
    let archived = send_with_headers(
        &case.router,
        Method::PATCH,
        &item,
        Some(&case.base.token),
        r#"{"status":"archived"}"#,
        &[(header::IF_MATCH, &tag)],
    )
    .await;
    assert_eq!(archived.status(), StatusCode::OK);
    let archived = json_body(archived).await;
    let current = archived["etag"].as_str().expect("archived tag");
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

async fn assert_retained_identity(case: &Case, key: &af_core::id::Uuid7) {
    let mut connection = case.base.db.acquire().await.expect("purge inspection");
    let remaining: i64 =
        sqlx::query_scalar("SELECT count(*) FROM fleet.fleets WHERE workspace_id=$1")
            .bind(&case.base.workspace)
            .fetch_one(&mut *connection)
            .await
            .expect("fleet census");
    assert_eq!(remaining, 0);
    let receipts: i64 =
        sqlx::query_scalar("SELECT count(*) FROM authoring.replay_receipts WHERE operation_id=$1")
            .bind(key)
            .fetch_one(&mut *connection)
            .await
            .expect("receipt census");
    assert_eq!(receipts, 1, "the receipt outlives the purged fleet");
    drop(connection);
}

#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn delete_removes_a_fleet_in_any_status() {
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
    let status = json_body(
        super::super::send(&case.router, Method::GET, &item, Some(&case.base.token), "").await,
    )
    .await["status"]
        .clone();
    assert!(
        status == "active" || status == "installing",
        "a live fleet, never archived first: {status}"
    );
    assert_eq!(
        remove(&case, &item, Some(&tag)).await.status(),
        StatusCode::NO_CONTENT,
        "delete takes a live fleet without an archive first"
    );
    let gone =
        super::super::send(&case.router, Method::GET, &item, Some(&case.base.token), "").await;
    assert_eq!(gone.status(), StatusCode::NOT_FOUND);
    let fleet = item.rsplit('/').next().expect("fleet id");
    let live: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM runner.leases WHERE fleet_id = $1::uuid AND status = 'active'",
    )
    .bind(fleet)
    .fetch_one(&mut *case.base.db.acquire().await.expect("lease census"))
    .await
    .expect("lease census");
    assert_eq!(live, 0, "no live lease outlives the deleted fleet");
    case.cleanup().await;
}
