//! A fleet's lifecycle is activate, archive or delete (`M249_002` §6), over the
//! real router: archive is terminal, keeps history, and no other status is
//! accepted.

use super::{
    super::{json_body, mint, send, send_with_headers},
    fixture::Case,
};
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
    for status in REMOVED {
        let body = format!(r#"{{"status":"{status}"}}"#);
        let refused = patch(&case, &item, &body, &tag).await;
        assert_eq!(refused.status(), StatusCode::BAD_REQUEST, "{status}");
    }

    let archived = patch(&case, &item, r#"{"status":"archived"}"#, &tag).await;
    assert_eq!(archived.status(), StatusCode::OK);
    let archived = json_body(archived).await;
    assert_eq!(archived["status"], "archived");
    let current = archived["etag"].as_str().expect("archived tag");
    for body in [r#"{"status":"active"}"#, r#"{"status":"archived"}"#] {
        let refused = patch(&case, &item, body, current).await;
        assert!(
            refused.status().is_client_error(),
            "an archived fleet refuses {body}: {}",
            refused.status()
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
    let fleet = item.rsplit('/').next().expect("fleet id");
    let live: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM runner.leases WHERE fleet_id = $1::uuid AND status = 'active'",
    )
    .bind(fleet)
    .fetch_one(&mut *case.base.db.acquire().await.expect("lease census"))
    .await
    .expect("lease census");
    assert_eq!(live, 0, "an archived fleet holds no live lease");
    case.cleanup().await;
}
