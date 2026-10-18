//! Applied setup changes are durable while the old immutable revision remains intact.

use super::{Fleet, Method, StatusCode, json_body, mint, send};

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn an_improvement_advances_only_setup_and_replays_after_the_private_draft_closes() {
    let fleet = Fleet::create().await;
    let (draft, request) = fleet.improvement().await;
    let key = mint();
    let tag = fleet.saved["setup_etag"].as_str().expect("base setup tag");
    let response = fleet.apply(key.as_str(), tag, &request).await;
    let status = response.status();
    let applied = json_body(response).await;
    assert_eq!(status, StatusCode::CREATED, "{applied}");
    assert_eq!(applied["setup_revision"], "2");
    let current = fleet.current().await;
    assert_eq!(current["id"], fleet.saved["id"]);
    assert_eq!(current["name"], fleet.saved["name"]);
    assert_eq!(current["status"], "active");
    assert_eq!(current["needs_re_review"], false);
    assert_ne!(current["definition"], fleet.saved["definition"]);
    assert_eq!(current["setup_etag"], applied["setup_etag"]);
    assert_eq!(fleet.revision_count().await, 2);
    assert_eq!(
        original_instructions(&fleet).await,
        fleet.saved["definition"]["instructions_md"]
            .as_str()
            .expect("saved instructions")
    );
    let closed = send(
        &fleet.case.router,
        Method::GET,
        &draft,
        Some(&fleet.case.base.token),
        "",
    )
    .await;
    assert_eq!(json_body(closed).await["state"], "closed");
    let replay = fleet.apply(key.as_str(), tag, &request).await;
    assert_eq!(replay.status(), StatusCode::CREATED);
    assert_eq!(json_body(replay).await["setup_revision"], "2");
    assert_eq!(fleet.revision_count().await, 2);
    assert_eq!(
        library_count(&fleet).await,
        0,
        "Apply has no implicit Library publication"
    );
    fleet.case.cleanup().await;
}

/// The instructions revision 1 still stores, read straight from the table.
async fn original_instructions(fleet: &Fleet) -> String {
    sqlx::query_scalar("SELECT definition->>'instructions_md' FROM fleet.revisions WHERE fleet_id=$1::uuid AND revision=1")
        .bind(fleet.saved["id"].as_str().expect("fleet ID")).fetch_one(&mut *fleet.case.base.db.acquire().await.expect("old setup connection"))
        .await.expect("immutable original setup")
}

/// How many Library rows the fleet's tenant owns.
async fn library_count(fleet: &Fleet) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM authoring.library_entries WHERE tenant_id=$1::uuid")
        .bind(fleet.case.base.tenant.as_str())
        .fetch_one(
            &mut *fleet
                .case
                .base
                .db
                .acquire()
                .await
                .expect("publication connection"),
        )
        .await
        .expect("publication count")
}
