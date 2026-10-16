//! Only the owning account deletes its library entry. Another account cannot
//! delete it or learn that it exists: through its own workspace the entry is
//! 404, and through the owner's workspace the workspace guard answers 403 for a
//! real entry and an absent one alike. The entry and its history stay in place.

use super::{Case, Fixture, Method, StatusCode, header, json_body, mint, send};
use afd_core::error_code::{self, AUTHORING_NOT_FOUND};

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn another_account_cannot_delete_or_detect_an_entry_and_the_owner_keeps_its_history() {
    let case = Case::create().await;
    let published = case.publish(mint().as_str()).await;
    assert_eq!(published.status(), StatusCode::CREATED);
    let library = published.headers()[header::LOCATION]
        .to_str()
        .expect("Library location")
        .to_owned();
    let id = json_body(published).await["library_id"]
        .as_str()
        .expect("Library id")
        .to_owned();
    let foreign = Fixture::create().await;
    let router = foreign.router();
    let own_path = format!("/v1/workspaces/{}/fleet-libraries/{id}", foreign.workspace);
    let refused = send(&router, Method::DELETE, &own_path, Some(&foreign.token), "").await;
    assert_eq!(refused.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        json_body(refused).await["error_code"],
        AUTHORING_NOT_FOUND.as_str()
    );
    // The owner's workspace path stops at the workspace guard, before the
    // library is looked up, so its answer names no library either way.
    let crossed = send(&router, Method::DELETE, &library, Some(&foreign.token), "").await;
    assert_eq!(crossed.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        json_body(crossed).await["error_code"],
        error_code::AUTH_FORBIDDEN.as_str()
    );
    let absent = format!(
        "/v1/workspaces/{}/fleet-libraries/{}",
        case.base.workspace,
        mint()
    );
    let unknown = send(&router, Method::DELETE, &absent, Some(&foreign.token), "").await;
    assert_eq!(
        unknown.status(),
        StatusCode::FORBIDDEN,
        "same answer, no lookup"
    );
    assert_eq!(case.get(&library).await.status(), StatusCode::OK);
    let history = json_body(case.get(&format!("{library}/revisions")).await).await;
    assert_eq!(history["items"].as_array().expect("history").len(), 1);
    let removed = send(
        &case.router,
        Method::DELETE,
        &library,
        Some(&case.base.token),
        "",
    )
    .await;
    assert_eq!(removed.status(), StatusCode::NO_CONTENT);
    foreign.cleanup().await;
    case.base.cleanup().await;
}
