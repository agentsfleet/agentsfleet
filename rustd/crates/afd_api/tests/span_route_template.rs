//! Dimension 6.1 — a span carries the route template, never the raw path.
//!
//! Asserted by capturing real spans through a `tracing` subscriber rather than
//! by reading the middleware's source, because the property is about what
//! reaches an exporter. The negative half is the one that matters: no attribute
//! on any span may contain a value out of the request line.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

mod harness;
mod recorder;

use afd_observability::semconv;
use axum::body::Body;
use http::{Method, Request};
use tower::ServiceExt as _;

use self::harness::Fleet;
use self::recorder::{Recorder, SpanRecord};

/// Drives one request and returns every span it opened.
async fn spans_for(method: Method, path: &str) -> Vec<SpanRecord> {
    let recorder = Recorder::install();
    let router = Fleet::new().router();
    let request = Request::builder()
        .method(method)
        .uri(path)
        .body(Body::empty())
        .expect("the test request is well formed");
    let _response = router.oneshot(request).await.expect("axum is infallible");
    recorder.spans()
}

/// The span names the template, the method, and the status it answered.
#[tokio::test]
async fn test_span_route_template() {
    let spans = spans_for(Method::GET, "/healthz").await;
    let span = spans
        .iter()
        .find(|span| span.name == "http.server.request")
        .expect("a matched request opens a server span");

    assert_eq!(
        span.field(semconv::ATTR_HTTP_ROUTE).as_deref(),
        Some("/healthz")
    );
    assert_eq!(
        span.field(semconv::ATTR_HTTP_REQUEST_METHOD).as_deref(),
        Some("GET")
    );
    assert_eq!(
        span.field(semconv::ATTR_HTTP_RESPONSE_STATUS_CODE)
            .as_deref(),
        Some("200"),
        "the status is recorded after the handler answers, not left empty"
    );
}

/// The macro's literal field names are the semconv keys.
///
/// `tracing` takes field names as syntax, so two of the three cannot be written
/// as constants. This is what stops the literals in `trace.rs` and the
/// vocabulary in `afd_observability` drifting apart — the compiler cannot.
#[test]
fn test_span_fields_are_the_semconv_keys() {
    assert_eq!(semconv::ATTR_HTTP_ROUTE, "http.route");
    assert_eq!(semconv::ATTR_HTTP_REQUEST_METHOD, "http.request.method");
    assert_eq!(
        semconv::ATTR_HTTP_RESPONSE_STATUS_CODE,
        "http.response.status_code"
    );
}

/// A request that matched nothing opens no span at all.
///
/// A 404 has no template, and both alternatives are worse: a span carrying the
/// raw path is the leak this whole module exists to prevent, and one carrying a
/// placeholder makes every unmatched request look like every other while still
/// costing a series.
#[tokio::test]
async fn test_an_unmatched_request_opens_no_server_span() {
    // Workspace-SHAPED but not in the route table, which is the interesting
    // case: a path that looks like a template must not be granted one. The
    // fixture was `…/secrets` until §4 mounted that family; a fixture that is
    // a real-but-unserved route would go the same way when its section lands,
    // so this names a segment the table has never held.
    let spans = spans_for(Method::GET, "/v1/workspaces/secret-id/not-a-route").await;

    assert!(
        !spans.iter().any(|span| span.name == "http.server.request"),
        "an unmatched path must not reach the span layer, got {spans:?}"
    );
}

/// No span attribute ever carries a value from the request line.
///
/// The point of the whole dimension, stated as a search rather than a spot
/// check: a concrete path holds workspace, fleet and lease identifiers, and a
/// span attribute is exported to a third-party backend.
#[tokio::test]
async fn test_no_span_attribute_carries_a_concrete_path() {
    // A path that MATCHES and carries an identifier-looking segment would be
    // ideal; this binary serves only static templates, so the identifier is
    // smuggled in as a query string, which `MatchedPath` must also exclude.
    let spans = spans_for(Method::GET, "/readyz?workspace=0190-secret-tenant").await;

    for span in &spans {
        for (name, value) in &span.fields {
            assert!(
                !value.contains("0190-secret-tenant"),
                "span field {name} leaked a request-line value: {value}"
            );
            assert!(
                !value.contains('?'),
                "span field {name} carries a query string: {value}"
            );
        }
    }

    let server = spans
        .iter()
        .find(|span| span.name == "http.server.request")
        .expect("a matched request opens a server span");
    assert_eq!(
        server.field(semconv::ATTR_HTTP_ROUTE).as_deref(),
        Some("/readyz"),
        "the template is the mounted string, with no query and no parameters"
    );
}

/// A parameterised route reports its TEMPLATE, not the identifier in the path.
///
/// This is the assertion the dimension is actually about, and it needs a route
/// with a parameter to make. Every route this binary mounts today is static, so
/// for all of them the template and the raw path are the same string — which
/// means every other test in this file would pass unchanged against a layer
/// that emitted `uri().path()`. Mounting the layer over a parameterised route
/// is the only way to tell those two implementations apart.
#[tokio::test]
async fn test_a_parameterised_route_reports_its_template() {
    let recorder = Recorder::install();
    let router = axum::Router::new()
        .route(
            "/v1/workspaces/{workspace_id}/secrets/{secret_name}",
            axum::routing::get(|| async { "" }),
        )
        .route_layer(axum::middleware::from_fn(afd_api::router::trace_requests));

    let request = Request::builder()
        .method(Method::GET)
        .uri("/v1/workspaces/01900000-0000-7000-8000-00000000beef/secrets/stripe-key")
        .body(Body::empty())
        .expect("the test request is well formed");
    let _response = router.oneshot(request).await.expect("axum is infallible");

    let spans = recorder.spans();
    let span = spans
        .iter()
        .find(|span| span.name == "http.server.request")
        .expect("a matched request opens a server span");
    let route = span
        .field(semconv::ATTR_HTTP_ROUTE)
        .expect("the span carries a route");

    assert_eq!(
        route, "/v1/workspaces/{workspace_id}/secrets/{secret_name}",
        "the span must carry the template"
    );
    assert!(
        !route.contains("beef"),
        "the workspace identifier reached the span: {route}"
    );
    assert!(
        !route.contains("stripe-key"),
        "the secret's name reached the span: {route}"
    );
}
