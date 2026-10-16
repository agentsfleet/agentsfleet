//! Saving reuses only the verified publication of the unchanged candidate.

use super::{
    super::{json_body, mint, send, send_with_headers},
    fixture::Case,
};
use http::{Method, StatusCode, header};
use serde_json::{Value, json};

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn explicit_publication_is_reused_by_save_and_deletion_preserves_the_installed_snapshot() {
    let mut case = Case::create().await;
    case.fund().await;
    let (_, request) = case.request().await;
    let (library, published) = publish_candidate(&mut case, &request).await;
    let saved = case.save(mint().as_str(), &request).await;
    assert_eq!(saved.status(), StatusCode::CREATED);
    let item = saved.headers()[header::LOCATION]
        .to_str()
        .expect("fleet location")
        .to_owned();
    let saved = json_body(saved).await;
    assert_eq!(saved["status"], "active");
    assert_eq!(
        installed_pin(&case, saved["id"].as_str().expect("fleet identity")).await,
        published["revision_id"]
            .as_str()
            .expect("accepted revision")
    );
    assert_library_deletion_hides_it(&case, &library).await;
    let retained = send(&case.router, Method::GET, &item, Some(&case.base.token), "").await;
    assert_eq!(retained.status(), StatusCode::OK);
    let retained = json_body(retained).await;
    assert_eq!(retained["setup_revision"], saved["setup_revision"]);
    assert_eq!(retained["definition"], saved["definition"]);
    assert_eq!(retained["status"], "active");
    case.cleanup().await;
}

/// Publishes the Save request's candidate as a Library entry the fixture then
/// owns, and answers the entry's location and body.
async fn publish_candidate(case: &mut Case, request: &Value) -> (String, Value) {
    let libraries = format!("/v1/workspaces/{}/fleet-libraries", case.base.workspace);
    let published = send_with_headers(
        &case.router,
        Method::POST,
        &libraries,
        Some(&case.base.token),
        &json!({"draft_candidate_id":request["source"]["draft_candidate_id"],"label":"ARGO"})
            .to_string(),
        &[(
            header::HeaderName::from_static("idempotency-key"),
            mint().as_str(),
        )],
    )
    .await;
    assert_eq!(published.status(), StatusCode::CREATED);
    let library = published.headers()[header::LOCATION]
        .to_str()
        .expect("entry location")
        .to_owned();
    let published = json_body(published).await;
    case.base
        .own_library(published["library_id"].as_str().expect("Library identity"));
    (library, published)
}

/// The Library revision the saved fleet's current definition is pinned to.
async fn installed_pin(case: &Case, fleet: &str) -> String {
    sqlx::query_scalar("SELECT r.library_revision_id::text FROM core.fleets f JOIN core.fleet_definition_revisions r ON r.id=f.current_definition_revision_id WHERE f.id=$1::uuid")
        .bind(fleet).fetch_one(&mut *case.base.db.acquire().await.expect("pin connection")).await.expect("installed publication pin")
}

/// Deletes the Library entry and proves it no longer reads back.
async fn assert_library_deletion_hides_it(case: &Case, library: &str) {
    let removed = send(
        &case.router,
        Method::DELETE,
        library,
        Some(&case.base.token),
        "",
    )
    .await;
    assert_eq!(removed.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        send(
            &case.router,
            Method::GET,
            library,
            Some(&case.base.token),
            ""
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
}
