//! One server span per matched request, carrying the template and nothing else.
//!
//! # The whole point is what is absent
//!
//! `dispatch` in `http/server.zig` says it in a comment: "The raw path is
//! deliberately not carried into the span: it holds real workspace, fleet, and
//! lease identifiers. The matched route's template does." Both halves matter,
//! and for different reasons.
//!
//! Privacy: a concrete path is tenant data, and a span attribute is exported to
//! a third-party backend. Cardinality: a route dimension with one value per
//! request is not a dimension — it cannot be grouped, and it prices like a log.
//!
//! # Why the template comes from axum and not from the route table
//!
//! [`MatchedPath`] hands back the exact string the router was mounted with, and
//! the router is mounted from [`crate::route::Route`]'s own templates. So it IS
//! the table's template — read back from the place that matched it, rather than
//! looked up a second time by a key that could disagree.
//!
//! # Why an unmatched request gets no span
//!
//! This is a `route_layer`, so it runs only when a route matched. A 404 has no
//! template by definition, and the alternatives are both worse: a span with the
//! raw path is the leak this module exists to prevent, and a span with a
//! constant placeholder buys a dimension where every unmatched request looks
//! like every other. Zig arrives at the same place by 404-ing before it opens a
//! trace at all.

use afd_observability::semconv;
use axum::extract::{MatchedPath, Request};
use axum::middleware::Next;
use axum::response::Response;
use tracing::Instrument as _;

/// The template for a request that matched nothing.
///
/// Unreachable through the router, which mounts this as a `route_layer`. It
/// exists so reading the extension is total, and it is a constant rather than
/// the raw path so that even the unreachable arm cannot leak one.
const UNMATCHED: &str = "unmatched";

/// Wraps one matched request in a server span.
///
/// Public because the property it carries — a template, never a path — is only
/// PROVABLE against a route that has parameters, and this binary mounts none
/// yet. A test mounts it over `/v1/workspaces/{workspace_id}` and reads the
/// span back; without that, every assertion about templates would be satisfied
/// just as well by emitting the raw path, since for a static route the two
/// strings are equal. It is also what §7 and the milestones after it mount.
///
/// The names in the macro are string literals because `tracing`'s macros take
/// field names as SYNTAX, not as values — a constant cannot be substituted
/// there. [`tracing::Span::record`] does take a value, so the status field
/// below names its constant directly; the other two are pinned by
/// `test_span_fields_are_the_semconv_keys` instead, which is the closest a test
/// can get to the guarantee the compiler gives that one.
pub async fn record(request: Request, next: Next) -> Response {
    // Read before the request is moved. `MatchedPath` is present because this
    // layer only runs on a matched route; the fallback keeps the read total
    // rather than describing a state the mounting makes unreachable.
    let template = request
        .extensions()
        .get::<MatchedPath>()
        .map_or(UNMATCHED, MatchedPath::as_str)
        .to_owned();
    let method = request.method().to_string();

    let span = tracing::info_span!(
        "http.server.request",
        "http.route" = template,
        "http.request.method" = method,
        // Declared empty and filled once the handler answers, so the field
        // belongs to the span from the moment it opens rather than appearing
        // on it late.
        "http.response.status_code" = tracing::field::Empty,
    );

    // `Instrument` rather than an entered guard: the inner future crosses await
    // points, and a guard held across one attributes another task's work to
    // this span.
    let response = next.run(request).instrument(span.clone()).await;

    span.record(
        semconv::ATTR_HTTP_RESPONSE_STATUS_CODE,
        response.status().as_u16(),
    );
    response
}
