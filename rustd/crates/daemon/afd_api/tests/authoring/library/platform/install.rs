//! A platform slug resolves one reviewed immutable revision during Save; the
//! saved fleet copies that content, so withdrawing the card later takes its
//! revisions and leaves the fleet's setup intact (spec Dimension 4.3).

use super::*;

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn reviewed_slug_save_copies_content_and_withdrawal_preserves_the_installed_setup() {
    let mut case = Case::create().await;
    case.curate(&json!({"published":true})).await;
    let library = json_body(case.inner.get(&case.tenant_path()).await).await;
    let (path, saved) = save_from_slug(&case, &library).await;
    assert_eq!(saved["definition"], library["head"]["definition"]);
    let revision = case.staged["revision_id"]
        .as_str()
        .expect("accepted revision")
        .to_owned();
    assert!(snapshot_exists(&case, &revision).await);
    case.curate(&json!({"published":false})).await;
    assert_eq!(case.withdraw().await.status(), StatusCode::NO_CONTENT);
    let retained = case.inner.get(&path).await;
    assert_eq!(retained.status(), StatusCode::OK);
    assert_eq!(json_body(retained).await["definition"], saved["definition"]);
    assert!(
        !snapshot_exists(&case, &revision).await,
        "deleting the card deletes its revisions"
    );
    case.inner.base.cleanup().await;
}

/// Saves a fleet from the published slug's reviewed revision and answers the
/// saved fleet's location and body.
async fn save_from_slug(case: &Case, library: &Value) -> (String, Value) {
    let body = json!({"source":{"kind":"library","library_id":case.slug,"revision_id":case.staged["revision_id"]},
        "name":case.slug,"review_digest":library["review"]["review_digest"],"start":{"kind":"none"}});
    let response = send_with_headers(
        &case.inner.router,
        Method::POST,
        &format!("/v1/workspaces/{}/fleets", case.inner.base.workspace),
        Some(&case.inner.base.token),
        &body.to_string(),
        &[(
            header::HeaderName::from_static("idempotency-key"),
            mint().as_str(),
        )],
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let path = response.headers()[header::LOCATION]
        .to_str()
        .expect("saved fleet path")
        .to_owned();
    (path, json_body(response).await)
}

/// Whether the Library revision row `revision` still exists.
async fn snapshot_exists(case: &Case, revision: &str) -> bool {
    sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM authoring.library_revisions WHERE id=$1::uuid)")
        .bind(revision)
        .fetch_one(
            &mut *case
                .inner
                .base
                .db
                .acquire()
                .await
                .expect("snapshot connection"),
        )
        .await
        .expect("retained snapshot")
}
