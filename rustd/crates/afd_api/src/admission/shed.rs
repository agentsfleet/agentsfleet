//! The 429 a full instance answers with, and the headers that make it useful.
//!
//! A shed says nothing about the request, because at the moment it is written
//! nothing about the request has been read — no credential, no body, no path
//! parameters. What it carries instead is the shape of the ceiling it hit:
//! `Retry-After` says when to come back, and the `X-RateLimit-*` trio says what
//! the limit was and when it resets. That is the whole of what a client can act
//! on, and it is exactly what `http/server.zig`'s `respondBackpressureShed`
//! writes.

use afd_core::clock::{self, UnixMillis};
use afd_core::error_code;
use axum::extract::Request;
use axum::response::{IntoResponse, Response};
use http::{HeaderName, HeaderValue, header};

use super::Admission;
use crate::envelope::ProblemResponse;
use crate::request_id::RequestId;

/// The sentence a shed carries. `error_registry.zig`'s `MSG_API_BACKPRESSURE`.
pub const SHED_DETAIL: &str = "Server is at its in-flight request ceiling";

/// How long a shed caller is asked to wait before retrying.
///
/// `handlers/common.zig`'s `RETRY_AFTER_BRIEF_SECONDS`. Brief on purpose: the
/// ceiling is in-flight CONCURRENCY, not a quota, so the slot a caller wants is
/// freed by whichever request finishes next rather than by a window rolling
/// over.
pub const RETRY_AFTER_SECONDS: i64 = 1;

/// How many concurrent requests this instance admits.
pub const HEADER_RATELIMIT_LIMIT: HeaderName = HeaderName::from_static("x-ratelimit-limit");

/// How many of those are free — always `0` on a shed, by definition.
pub const HEADER_RATELIMIT_REMAINING: HeaderName = HeaderName::from_static("x-ratelimit-remaining");

/// When the caller should expect a slot, as Unix epoch seconds.
pub const HEADER_RATELIMIT_RESET: HeaderName = HeaderName::from_static("x-ratelimit-reset");

/// Nothing is free, and there is nothing to report as free.
const REMAINING_NONE: usize = 0;

/// The refusal for a request that arrived at a full instance.
pub(super) fn response(admission: &Admission, request: &Request) -> Response {
    // Minted once and used twice — the log line and the envelope carry the SAME
    // id, so an operator reading the log can find the client's screenshot. The
    // Zig shed logs no id at all, which leaves its two records uncorrelated.
    let request_id = RequestId::mint();
    // Hoisted rather than written inline in the macro: `tracing`'s `log`
    // feature compiles a SECOND copy of every field expression for the `log`
    // bridge, and llvm-cov reports the copy that never runs as uncovered.
    let limit = admission.limit().get();
    let path = request.uri().path();
    let request_id_field = request_id.as_str();
    let code = error_code::API_BACKPRESSURE.as_str();
    tracing::warn!(
        error_code = code,
        request_id = request_id_field,
        limit,
        // The raw path, as the Zig shed logs it. It is NOT what §6 will put on
        // a span — a path carries workspace and fleet identifiers, and a span
        // attribute is exported to a backend — but an operator reading their
        // own logs during a storm needs to know which endpoint is storming.
        path,
        event = "request_shed",
        "request shed at the in-flight ceiling"
    );

    (
        [
            (header::RETRY_AFTER, HeaderValue::from(RETRY_AFTER_SECONDS)),
            (HEADER_RATELIMIT_LIMIT, HeaderValue::from(limit)),
            (
                HEADER_RATELIMIT_REMAINING,
                HeaderValue::from(REMAINING_NONE),
            ),
            (
                HEADER_RATELIMIT_RESET,
                HeaderValue::from(reset_epoch_seconds(clock::now())),
            ),
        ],
        ProblemResponse::new(error_code::API_BACKPRESSURE, SHED_DETAIL, request_id),
    )
        .into_response()
}

/// When a shed caller should expect the instance to have room.
///
/// Pure, taking the instant rather than reading the clock, which is what
/// `afd_core::clock` asks callers to do wherever the decision can be handed a
/// value. Signed and saturating: `respondBackpressureShed` casts its sum to
/// `u64`, so a host whose clock is set before 1970 wraps there into a reset
/// time roughly 584 billion years away.
fn reset_epoch_seconds(now: UnixMillis) -> i64 {
    now.as_seconds().saturating_add(RETRY_AFTER_SECONDS)
}
