use af_core::error_code;
use axum::http::StatusCode;
use serde_json::{Value, json};

use super::{BOTH, Fixtures, Reply, Scenario};

const URL: &str = "https://1.1.1.1/guide";

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_web_call_that_may_not_leave_reaches_no_vendor() {
    let f = Fixtures::create().await;
    let mut s = Scenario::new(&f, BOTH, vec![]).await;
    f.set_network(&s.live.ready.fleet, "restricted", false)
        .await;
    assert_eq!(
        s.search().await.expect_err("private search").code(),
        error_code::WEB_NETWORK_RESTRICTED
    );
    assert_eq!(
        s.extract(URL).await.expect_err("private extract").code(),
        error_code::WEB_NETWORK_RESTRICTED
    );
    f.set_network(&s.live.ready.fleet, "public", false).await;
    for url in [
        "https://169.254.169.254/",
        "https://localhost/",
        "http://example.org",
        "https://someone@example.org/private",
        "https://@1.1.1.1/private",
    ] {
        assert_eq!(
            s.extract(url).await.expect_err("invalid target").code(),
            error_code::WEB_INVALID_URL
        );
    }
    s.fence += 1;
    assert_eq!(
        s.extract(URL).await.expect_err("stale fence").code(),
        error_code::RUN_STALE_FENCING_TOKEN
    );
    assert_eq!(s.reservations(&f).await, 0);
    assert!(s.vendor.seen().is_empty());
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_extract_returns_the_pages_markdown() {
    let f = Fixtures::create().await;
    let page = Reply::new(
        StatusCode::OK,
        json!({"results":[{"url":URL,"full_content":"## The page"}],"errors":[]}),
    );
    let error = |status| {
        Reply::new(
            StatusCode::OK,
            json!({"results":[],"errors":[{"url":URL,"http_status_code":status}]}),
        )
    };
    let invalid = Reply::new(
        StatusCode::OK,
        json!({"results":[{"url":URL,"excerpts":["wrong field"]}],"errors":[]}),
    );
    let s = Scenario::new(&f, BOTH, vec![page, error(404), invalid, error(429)]).await;
    let result = s.extract(URL).await.expect("extract returns full page");
    assert_eq!(result.content, "## The page");
    assert_eq!(result.url, URL);
    assert_eq!(s.reservations(&f).await, 1);
    assert_eq!(
        s.vendor.seen()[0].body["advanced_settings"]["full_content"],
        true
    );
    for code in [
        error_code::WEB_PAGE_UNAVAILABLE,
        error_code::WEB_EXTRACT_INVALID_RESPONSE,
    ] {
        let error = s
            .extract(URL)
            .await
            .expect_err("invalid or definitive result");
        assert_eq!(error.code(), code);
        assert!(
            !error
                .web_failure()
                .expect("typed failure")
                .local_read_permitted()
        );
    }
    let retry = s
        .extract(URL)
        .await
        .expect_err("temporary per-page failure");
    assert!(
        retry
            .web_failure()
            .expect("web failure")
            .local_read_permitted()
    );
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_fleet_restricted_during_extraction_cannot_fall_back() {
    let f = Fixtures::create().await;
    let mut restrict = Reply::new(StatusCode::SERVICE_UNAVAILABLE, Value::Null);
    restrict.restrict = true;
    let s = Scenario::new(&f, BOTH, vec![restrict]).await;
    let refusal = s
        .extract(URL)
        .await
        .expect_err("policy must be checked after vendor outage");
    assert_eq!(refusal.code(), error_code::WEB_NETWORK_RESTRICTED);
    assert!(
        !refusal
            .web_failure()
            .expect("policy refusal")
            .local_read_permitted()
    );
    assert_eq!(s.reservations(&f).await, 1);
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_failed_reservation_sends_nothing_and_confers_no_fallback() {
    let f = Fixtures::create_isolated().await;
    let s = Scenario::new(&f, BOTH, vec![]).await;
    let admin = f
        .shared_lane()
        .open(afd_db::config::DbRole::Migrator, &[])
        .await;
    let mut connection = admin.acquire().await.expect("isolated table owner");
    sqlx::query("REVOKE INSERT ON thread.run_web_calls FROM api_runtime")
        .execute(&mut *connection)
        .await
        .expect("inject a reservation failure");
    drop(connection);
    let error = s.extract(URL).await.expect_err("no ledger reservation");
    assert_eq!(error.code(), error_code::WEB_RESERVATION_UNAVAILABLE);
    assert!(
        !error
            .web_failure()
            .expect("reservation failure")
            .local_read_permitted()
    );
    assert_eq!(
        s.search().await.expect_err("search cannot reserve").code(),
        error_code::WEB_RESERVATION_UNAVAILABLE
    );
    assert!(s.vendor.seen().is_empty());
    assert_eq!(s.reservations(&f).await, 0);
    admin.close().await;
    f.cleanup().await;
}
