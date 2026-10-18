//! A purged fleet leaves a bounded, content-free Apply receipt.

use super::{Fleet, Method, StatusCode, header, json_body, mint, send_with_headers};
use afd_core::error_code;

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn retrying_apply_after_purge_returns_gone_without_reviving_the_fleet() {
    let fleet = Fleet::create().await;
    let (_, request) = fleet.improvement().await;
    let key = mint();
    let base = fleet.saved["setup_etag"].as_str().expect("base setup tag");
    let applied = fleet.apply(key.as_str(), base, &request).await;
    assert_eq!(applied.status(), StatusCode::CREATED);
    kill_and_purge(&fleet).await;

    let replay = fleet.apply(key.as_str(), base, &request).await;
    assert_eq!(replay.status(), StatusCode::GONE);
    assert_eq!(
        json_body(replay).await["error_code"],
        error_code::AUTHORING_GONE.as_str()
    );
    assert_eq!(
        fleet.revision_count().await,
        0,
        "no revision survives or returns"
    );
    assert_only_the_tombstone_remains(&fleet, key.as_str()).await;
    fleet.case.cleanup().await;
}

/// Kills the fleet, then purges it under the killed resource's tag.
async fn kill_and_purge(fleet: &Fleet) {
    let current = fleet.current().await;
    let tag = current["etag"].as_str().expect("fleet tag");
    let killed = fleet.patch_resource(r#"{"status":"killed"}"#, tag).await;
    assert_eq!(killed.status(), StatusCode::OK);
    let killed = json_body(killed).await;
    let purged = send_with_headers(
        &fleet.case.router,
        Method::DELETE,
        &fleet.item,
        Some(&fleet.case.base.token),
        "",
        &[(
            header::IF_MATCH,
            killed["etag"].as_str().expect("killed tag"),
        )],
    )
    .await;
    assert_eq!(purged.status(), StatusCode::NO_CONTENT);
}

/// The replay recreated no fleet row, and the Apply receipt survives as a
/// tombstone that expires exactly one tombstone window after the purge.
async fn assert_only_the_tombstone_remains(fleet: &Fleet, key: &str) {
    let mut connection = fleet
        .case
        .base
        .db
        .acquire()
        .await
        .expect("purge inspection");
    let fleets: i64 = sqlx::query_scalar("SELECT count(*) FROM fleet.fleets WHERE id=$1::uuid")
        .bind(fleet.saved["id"].as_str().expect("fleet ID"))
        .fetch_one(&mut *connection)
        .await
        .expect("fleet census");
    assert_eq!(fleets, 0, "the replay recreated nothing");
    let receipts: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM authoring.replay_receipts WHERE operation_id=$1::uuid",
    )
    .bind(key)
    .fetch_one(&mut *connection)
    .await
    .expect("receipt census");
    assert_eq!(receipts, 1, "the Apply receipt outlives the purged fleet");
}
