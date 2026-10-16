//! Replay identity and strong tags refuse a second mutation over the real router.

use super::{
    super::{json_body, mint, send_with_headers},
    fixture::Case,
};
use afd_core::error_code;
use http::{Method, StatusCode, header};

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn stale_tags_and_changed_replay_bodies_cannot_rename_or_resurrect() {
    let case = Case::create().await;
    case.fund().await;
    let (_, mut request) = case.request().await;
    let key = mint();
    let response = case.save(key.as_str(), &request).await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let item = response.headers()[header::LOCATION]
        .to_str()
        .expect("fleet location")
        .to_owned();
    let tag = response.headers()[header::ETAG]
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
    assert_eq!(killed["status"], "killed");
    let stale = send_with_headers(
        &case.router,
        Method::PATCH,
        &item,
        Some(&case.base.token),
        r#"{"name":"stale-edit"}"#,
        &[(header::IF_MATCH, &tag)],
    )
    .await;
    assert_eq!(stale.status(), StatusCode::PRECONDITION_FAILED);
    assert_eq!(json_body(stale).await["etag"], killed["etag"]);
    let replay = case.save(key.as_str(), &request).await;
    assert_eq!(replay.status(), StatusCode::CREATED);
    assert_eq!(json_body(replay).await["status"], "killed");
    request["name"] = "changed-key-body".into();
    let conflict = case.save(key.as_str(), &request).await;
    assert_eq!(conflict.status(), StatusCode::CONFLICT);
    assert_eq!(
        json_body(conflict).await["error_code"],
        error_code::AUTHORING_REPLAY.as_str()
    );
    case.cleanup().await;
}
