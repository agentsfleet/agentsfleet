//! Paid web dispatches on the same isolated HTTP fixture as native decisions.

use std::time::Duration;

use af_providers::web::{Exa, Parallel, Vendor, WebFailure};
use reqwest::Url;
use serde_json::json;

use super::fixture::{Reply, Server, credential};

const BUDGET: Duration = Duration::from_secs(2);
const PAGE: &str = "https://example.org/guide";

fn endpoint(server: &Server, path: &str) -> Url {
    format!("{}{path}", server.base)
        .parse()
        .expect("fixture URL")
}

fn parallel(server: &Server) -> Parallel {
    Parallel::with_endpoints(
        server.client.clone(),
        endpoint(server, "/v1/search"),
        endpoint(server, "/v1/extract"),
    )
}

#[tokio::test]
async fn a_fast_search_and_exa_highlights_are_each_a_single_paid_request() {
    let first = json!({"results": [{
        "url": " https://example.org/one two/<item> ", "publish_date": "2026-11-27", "excerpts": ["first", "second"],
    }]});
    let second = json!({"results": [{"url": PAGE, "title": "Guide", "highlights": ["one"]}]});
    let mut server = Server::start(vec![Reply::Json(first), Reply::Json(second)]).await;
    let parallel = parallel(&server);
    let exa = Exa::with_endpoint(server.client.clone(), endpoint(&server, "/search"));
    let key = credential();
    let query = "migration guides";
    let rows = parallel
        .search(&key, query, BUDGET)
        .await
        .expect("Parallel search");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].url, "https://example.org/one%20two/%3Citem%3E");
    assert_eq!(rows[0].published.as_deref(), Some("2026-11-27"));
    assert_eq!(rows[0].excerpts, ["first", "second"]);
    let seen = server.next().await;
    assert_eq!(seen.path, "/v1/search");
    assert_eq!(seen.headers["x-api-key"], key.expose());
    assert_eq!(
        seen.body,
        json!({
            "search_queries": [query], "mode": "fast", "advanced_settings": {"max_results": 10}
        })
    );
    let rows = exa.search(&key, query, BUDGET).await.expect("Exa search");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].title.as_deref(), Some("Guide"));
    assert_eq!(rows[0].excerpts, ["one"]);
    let seen = server.next().await;
    assert_eq!(seen.path, "/search");
    assert_eq!(seen.headers["x-api-key"], key.expose());
    assert_eq!(
        seen.body,
        json!({
            "query": query, "type":"auto", "numResults":10, "contents":{"highlights":true}
        })
    );
    assert_eq!(server.unconsumed(), 0);
    server.shutdown().await;
}

#[tokio::test]
async fn extraction_requires_one_matching_full_content_or_page_status() {
    let full = json!({"url": PAGE, "full_content":"# Migration\nA guide."});
    let bad = json!({"url": PAGE, "error_type":"http_error", "http_status_code":404, "content":"do not echo this"});
    let mut server = Server::start(vec![
        Reply::Json(json!({"results":[full], "errors":[]})),
        Reply::Json(json!({"results":[], "errors":[bad]})),
        Reply::Json(json!({"results":[], "errors":[{"url":PAGE,"error_type":"http_error","http_status_code":503}]})),
        Reply::Json(json!({"results":[full], "errors":[bad]})),
        Reply::Json(json!({"results":[{"url":PAGE, "excerpts":["snippet"]}], "errors":[]})),
        Reply::Json(json!({"results":[{"url":"https://elsewhere.test/?signed=private", "full_content":"wrong"}], "errors":[]})),
    ]).await;
    let vendor = parallel(&server);
    let key = credential();
    let page = vendor
        .extract(&key, PAGE, BUDGET)
        .await
        .expect("full content");
    assert_eq!(page.content, "# Migration\nA guide.");
    assert_eq!(page.url, PAGE);
    let seen = server.next().await;
    assert_eq!(seen.path, "/v1/extract");
    assert_eq!(seen.headers["x-api-key"], key.expose());
    assert_eq!(
        seen.body,
        json!({"urls":[PAGE], "advanced_settings":{"full_content":true}})
    );
    for expected in [
        WebFailure::Page { status: 404 },
        WebFailure::Page { status: 503 },
        WebFailure::InvalidResponse,
        WebFailure::InvalidResponse,
        WebFailure::InvalidResponse,
    ] {
        assert_eq!(
            vendor
                .extract(&key, PAGE, BUDGET)
                .await
                .expect_err("invalid page"),
            expected
        );
        assert_eq!(server.next().await.path, "/v1/extract");
    }
    assert_eq!(server.unconsumed(), 0);
    server.shutdown().await;
}

#[tokio::test]
async fn unsafe_rows_and_refusals_expose_only_their_bounded_reason() {
    let key = credential();
    let mut server = Server::start(vec![
        Reply::Json(json!({"results":[{"url":"javascript:alert(1)", "excerpts":[]}]})),
        Reply::Json(json!({"results":[{"url": PAGE, "excerpts":[key.expose()]}]})),
        Reply::PrivateFailure(key.expose().to_owned()),
        Reply::Status(429, Some("30")),
    ])
    .await;
    let vendor = parallel(&server);
    for expected in [
        WebFailure::InvalidResponse,
        WebFailure::InvalidResponse,
        WebFailure::Status {
            status: 500,
            retry_after: None,
        },
        WebFailure::Status {
            status: 429,
            retry_after: Some(Duration::from_secs(30)),
        },
    ] {
        let failure = vendor
            .search(&key, "question", BUDGET)
            .await
            .expect_err("refusal");
        assert_eq!(failure, expected);
        assert!(!failure.to_string().contains(key.expose()));
        assert!(!format!("{failure:?}").contains(key.expose()));
        assert_eq!(server.next().await.path, "/v1/search");
    }
    assert_eq!(server.unconsumed(), 0);
    server.shutdown().await;
}

#[tokio::test]
async fn positional_arrays_cannot_authorize_a_local_read() {
    let mut server = Server::start(vec![
        Reply::Json(
            json!([[], [{"url": PAGE, "error_type":"http_error", "http_status_code": 503}]]),
        ),
        Reply::Json(json!({"results": [], "errors": [[PAGE, "http_error", 503]]})),
        Reply::Json(json!({"results": [[PAGE, null, "page text"]], "errors": []})),
    ])
    .await;
    let vendor = parallel(&server);
    for _ in 0..3 {
        let failure = vendor
            .extract(&credential(), PAGE, BUDGET)
            .await
            .expect_err("object required");
        assert_eq!(failure, WebFailure::InvalidResponse);
        assert_eq!(server.next().await.path, "/v1/extract");
    }
    server.shutdown().await;
}

#[tokio::test]
async fn recorded_parallel_extracts_keep_the_requested_source_and_classify_page_failures() {
    #[derive(serde::Deserialize)]
    struct Cases {
        cases: Vec<Case>,
    }
    #[derive(serde::Deserialize)]
    struct Case {
        requested_url: String,
        response: serde_json::Value,
    }
    let recording = include_str!("fixtures/parallel_extract.json");
    let cases: Cases = serde_json::from_str(recording).expect("sanitized Parallel fixture");
    let mut server = Server::start(
        cases
            .cases
            .iter()
            .map(|case| Reply::Json(case.response.clone()))
            .collect(),
    )
    .await;
    let vendor = parallel(&server);
    let key = credential();
    let expected = [
        Ok(()),
        Ok(()),
        Err(WebFailure::Transport),
        Err(WebFailure::Page { status: 429 }),
    ];
    assert_eq!(cases.cases.len(), expected.len());
    for (case, expected) in cases.cases.iter().zip(expected) {
        let result = vendor
            .extract(&key, &case.requested_url, BUDGET)
            .await
            .map(|page| {
                assert_eq!(page.url, case.requested_url);
                assert_eq!(page.content, "# Fixture page");
            });
        assert_eq!(result, expected);
        assert_eq!(server.next().await.path, "/v1/extract");
    }
    server.shutdown().await;
}

#[tokio::test]
async fn incomplete_or_unknown_per_page_errors_keep_fallback_closed() {
    for item in [
        json!({"url":PAGE,"http_status_code":503}),
        json!({"url":PAGE,"error_type":"connect_error"}),
        json!({"url":PAGE,"error_type":"http_error","http_status_code":null}),
        json!({"url":PAGE,"error_type":"unknown","http_status_code":null}),
    ] {
        let server = Server::start(vec![Reply::Json(json!({"results":[],"errors":[item]}))]).await;
        let failure = parallel(&server)
            .extract(&credential(), PAGE, BUDGET)
            .await
            .expect_err("no usable error");
        assert_eq!(failure, WebFailure::InvalidResponse);
    }
}
