//! The in-flight ceiling's default and the response a full instance answers.
//!
//! The router enforces it once over the API route group with tower's stock
//! layers: `GlobalConcurrencyLimitLayer` under `LoadShedLayer`, so a full
//! instance refuses at once rather than queueing, under axum's
//! `HandleErrorLayer`, which answers [`shed`]. The permit lives in the
//! response future: it is released when the handler has produced its
//! response, before the body is sent. Probes are never limited, and streams
//! are capped by `afd_sse::Live`.

use std::num::NonZeroUsize;

use af_core::clock;
use af_core::error_code;
use axum::response::{IntoResponse, Response};
use http::{HeaderName, HeaderValue, Uri, header};

use crate::envelope::ProblemResponse;
use crate::request_id::RequestId;

/// Requests one instance serves at once when nothing configures otherwise.
pub const DEFAULT_MAX_IN_FLIGHT: NonZeroUsize = NonZeroUsize::new(256).unwrap();

/// The sentence a shed carries.
pub const SHED_DETAIL: &str = "Server is at its in-flight request ceiling";

/// How long a shed caller is asked to wait before retrying: the slot frees
/// when whichever request finishes next.
pub const RETRY_AFTER_SECONDS: i64 = 1;

/// How many concurrent requests this instance admits.
pub const HEADER_RATELIMIT_LIMIT: HeaderName = HeaderName::from_static("x-ratelimit-limit");

/// How many of those are free: always `0` on a shed.
pub const HEADER_RATELIMIT_REMAINING: HeaderName = HeaderName::from_static("x-ratelimit-remaining");

/// When the caller should expect a slot, as Unix epoch seconds.
pub const HEADER_RATELIMIT_RESET: HeaderName = HeaderName::from_static("x-ratelimit-reset");

/// The refusal for a request that arrived at a full instance: what
/// `HandleErrorLayer` answers for `LoadShedLayer`'s overload error.
#[must_use]
pub fn shed(limit: NonZeroUsize, uri: &Uri) -> Response {
    af_observability::producers::http::request_shed();
    let request_id = RequestId::current();
    let (limit, path) = (limit.get(), uri.path());
    let (request_id_field, code) = (request_id.as_str(), error_code::API_BACKPRESSURE.as_str());
    tracing::warn!(
        error_code = code,
        request_id = request_id_field,
        limit,
        path,
        event = "request_shed",
        "request shed at the in-flight ceiling"
    );
    (
        [
            (header::RETRY_AFTER, HeaderValue::from(RETRY_AFTER_SECONDS)),
            (HEADER_RATELIMIT_LIMIT, HeaderValue::from(limit)),
            (HEADER_RATELIMIT_REMAINING, HeaderValue::from_static("0")),
            (
                HEADER_RATELIMIT_RESET,
                HeaderValue::from(clock::now().as_second().saturating_add(RETRY_AFTER_SECONDS)),
            ),
        ],
        ProblemResponse::new(error_code::API_BACKPRESSURE, SHED_DETAIL, request_id),
    )
        .into_response()
}
