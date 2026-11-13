//! A fleet's lifecycle is activate, archive or delete, over the real router:
//! archive is terminal, ends the fleet's live work and keeps its history, and
//! no other status is accepted, so a fleet is never held between states.

use super::{
    super::{json_body, mint, send, send_with_headers},
    fixture::Case,
};
use af_core::error_code;
use http::{Method, StatusCode, header};

/// Statuses the lifecycle no longer has; each is a validation error.
const REMOVED: [&str; 3] = ["stopped", "killed", "paused"];

async fn patch(case: &Case, item: &str, body: &str, tag: &str) -> axum::response::Response {
    send_with_headers(
        &case.router,
        Method::PATCH,
        item,
        Some(&case.base.token),
        body,
        &[(header::IF_MATCH, tag)],
    )
    .await
}

#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn fleet_lifecycle_is_activate_archive_delete() {
    let mut case = Case::create().await;
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
    assert_eq!(
        json_body(saved).await["status"],
        "active",
        "archive starts from a live fleet"
    );
    let fleet =
        af_core::id::Uuid7::parse(item.rsplit('/').next().expect("fleet id")).expect("fleet id");
    let (_, lease) = case.live_lease(&fleet).await;
    for status in REMOVED {
        let body = format!(r#"{{"status":"{status}"}}"#);
        let refused = patch(&case, &item, &body, &tag).await;
        assert_eq!(refused.status(), StatusCode::BAD_REQUEST, "{status}");
        assert_eq!(
            json_body(refused).await["error_code"],
            error_code::INVALID_REQUEST.as_str(),
            "{status}"
        );
    }

    let archived = patch(&case, &item, r#"{"status":"archived"}"#, &tag).await;
    assert_eq!(archived.status(), StatusCode::OK);
    let archived = json_body(archived).await;
    assert_eq!(archived["status"], "archived");
    assert_eq!(
        super::fixture::lease_state(&case, &lease).await,
        Some(("active".to_owned(), true)),
        "the live lease is asked to cancel"
    );
    let current = archived["etag"].as_str().expect("archived tag");
    for body in [r#"{"status":"active"}"#, r#"{"status":"archived"}"#] {
        let refused = patch(&case, &item, body, current).await;
        assert_eq!(refused.status(), StatusCode::NOT_FOUND, "{body}");
        assert_eq!(
            json_body(refused).await["error_code"],
            error_code::AGENTSFLEET_NOT_FOUND.as_str(),
            "an archived fleet takes no further transition: {body}"
        );
    }

    let read = send(&case.router, Method::GET, &item, Some(&case.base.token), "").await;
    assert_eq!(
        read.status(),
        StatusCode::OK,
        "an archived fleet stays readable"
    );
    assert_eq!(json_body(read).await["status"], "archived");
    let history = format!("{item}/messages?event_type=activity");
    let events = send(
        &case.router,
        Method::GET,
        &history,
        Some(&case.base.token),
        "",
    )
    .await;
    assert_eq!(
        events.status(),
        StatusCode::OK,
        "its history stays readable"
    );
    case.cleanup().await;
}
