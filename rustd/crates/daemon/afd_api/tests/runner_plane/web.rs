//! A runner receives the vendor result or the precise instructions authorized by the daemon.
use std::time::Duration;

use af_core::error_code::{self, ErrorCode};
use af_wire::web::{ExtractResponse, SearchResponse, SearchResult};
use afd_auth::directory::Liveness;
use afd_fleet::run::web::Failure;
use axum::{Router, response::Response};
use http::{Method, StatusCode, header};
use serde_json::{Value, json};

use super::{RUN_ID, RUNNER_TOKEN};
use crate::harness::{Fleet, json_body, runner_id, send};

const PAGE: &str = "https://example.org/guide";
const SEARCH: (&str, &str) = (
    "searches",
    r#"{"fencing_token":7,"call_number":1,"query":"guide"}"#,
);
const EXTRACT: (&str, &str) = (
    "extracts",
    r#"{"fencing_token":7,"call_number":2,"url":"https://example.org/guide"}"#,
);

fn plane() -> Fleet {
    Fleet::new().with_runner(RUNNER_TOKEN, &runner_id(), Liveness::Live)
}

async fn call(router: &Router, (suffix, body): (&str, &str)) -> Response {
    let path = format!("/v1/runners/me/runs/{RUN_ID}/{suffix}");
    send(router, Method::POST, &path, Some(RUNNER_TOKEN), body).await
}

#[tokio::test]
async fn search_and_extract_return_the_vendor_source_and_page_as_json() {
    let search = SearchResponse {
        vendor: "exa".into(),
        results: vec![SearchResult {
            url: PAGE.into(),
            title: None,
            published: None,
            excerpts: vec!["excerpt".into()],
        }],
    };
    let page = ExtractResponse {
        vendor: "parallel".into(),
        url: PAGE.into(),
        title: Some("Guide".into()),
        content: "# Guide".into(),
    };
    let router = plane().with_web_replies(search, page).router();
    for (request, expected) in [
        (
            SEARCH,
            json!({"vendor":"exa","results":[{"url":PAGE,"excerpts":["excerpt"]}]}),
        ),
        (
            EXTRACT,
            json!({"vendor":"parallel","url":PAGE,"title":"Guide","content":"# Guide"}),
        ),
    ] {
        let reply = call(&router, request).await;
        assert_eq!(reply.status(), StatusCode::OK);
        assert_eq!(
            reply
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok()),
            Some("application/json")
        );
        assert!(reply.headers().get(header::RETRY_AFTER).is_none());
        assert_eq!(json_body(reply).await, expected);
    }
}

async fn assert_refusal(
    reply: Response,
    status: StatusCode,
    code: ErrorCode,
    wait: Option<&str>,
    local_read: Option<bool>,
) {
    assert_eq!(reply.status(), status);
    assert_eq!(
        reply
            .headers()
            .get(header::RETRY_AFTER)
            .and_then(|value| value.to_str().ok()),
        wait
    );
    let problem = json_body(reply).await;
    assert_eq!(
        problem.get("error_code").and_then(Value::as_str),
        Some(code.as_str())
    );
    assert_eq!(
        problem.get("local_read_permitted"),
        local_read.map(Value::Bool).as_ref()
    );
}

#[tokio::test]
async fn vendor_wait_rounds_up_without_authorizing_a_search_to_read_locally() {
    let router = plane()
        .with_run_refusal(|| Failure::Cooling(Duration::from_millis(2_001)).into())
        .router();
    assert_refusal(
        call(&router, SEARCH).await,
        StatusCode::SERVICE_UNAVAILABLE,
        error_code::WEB_VENDORS_COOLING,
        Some("3"),
        None,
    )
    .await;
}

#[tokio::test]
async fn only_a_temporary_reader_outage_authorizes_one_local_read() {
    let router = plane()
        .with_run_refusal(|| Failure::ExtractUnavailable(Some(Duration::from_secs(5))).into())
        .router();
    assert_refusal(
        call(&router, EXTRACT).await,
        StatusCode::BAD_GATEWAY,
        error_code::WEB_EXTRACT_UNAVAILABLE,
        Some("5"),
        Some(true),
    )
    .await;
    let router = plane()
        .with_run_refusal(|| Failure::Page(404).into())
        .router();
    assert_refusal(
        call(&router, EXTRACT).await,
        StatusCode::BAD_GATEWAY,
        error_code::WEB_PAGE_UNAVAILABLE,
        None,
        None,
    )
    .await;
}

#[tokio::test]
async fn ambiguous_reader_failures_never_grant_local_access_or_a_retry_time() {
    let cases = [
        (
            (|| Failure::InvalidUrl.into()) as fn() -> afd_fleet::Error,
            StatusCode::BAD_REQUEST,
            error_code::WEB_INVALID_URL,
        ),
        (
            || Failure::InvalidResponse.into(),
            StatusCode::BAD_GATEWAY,
            error_code::WEB_EXTRACT_INVALID_RESPONSE,
        ),
        (
            || Failure::Reservation.into(),
            StatusCode::SERVICE_UNAVAILABLE,
            error_code::WEB_RESERVATION_UNAVAILABLE,
        ),
        (
            || Failure::Unconfigured.into(),
            StatusCode::SERVICE_UNAVAILABLE,
            error_code::WEB_VENDOR_UNCONFIGURED,
        ),
        (
            || Failure::Deadline.into(),
            StatusCode::SERVICE_UNAVAILABLE,
            error_code::WEB_DEADLINE_EXCEEDED,
        ),
    ];
    for (refusal, status, code) in cases {
        let router = plane().with_run_refusal(refusal).router();
        assert_refusal(call(&router, EXTRACT).await, status, code, None, None).await;
    }
}

#[tokio::test]
async fn exhausted_search_vendors_report_each_status_without_a_local_read() {
    use afd_fleet::run::web::VendorName;
    let router = plane()
        .with_run_refusal(|| {
            Failure::Vendors(vec![
                (VendorName::Parallel, Some(502)),
                (VendorName::Exa, None),
            ])
            .into()
        })
        .router();

    let reply = call(&router, SEARCH).await;
    assert_eq!(reply.status(), StatusCode::BAD_GATEWAY);
    assert!(reply.headers().get(header::RETRY_AFTER).is_none());
    let problem = json_body(reply).await;
    assert_eq!(
        problem.get("error_code").and_then(Value::as_str),
        Some(error_code::WEB_VENDORS_FAILED.as_str())
    );
    assert_eq!(
        problem.get("detail").and_then(Value::as_str),
        Some(
            "The configured web vendors could not answer the search. parallel (HTTP 502). exa (no usable response)."
        )
    );
    assert!(problem.get("local_read_permitted").is_none());
}
