//! `POST /v1/runners/me/leases/{lease_id}/tool-calls` — each finished call,
//! in full, for an operator's "show all".
//!
//! Its own verb rather than a field of the report. A large record set beside an
//! uncapped reply could carry one request past the runner routes' body limit,
//! and a refused report loses the run's answer; a refused post loses only the
//! records. The runner posts before it reports, and nothing about settlement
//! waits on a post.
//!
//! Fenced as the memory push is (`afd_fleet::lease::tool_detail`): the lease
//! must be this runner's and live, and a holder a reclaim has superseded is
//! refused. Each record is read on its own, so one of the wrong shape or over
//! a bound is counted in the answer and the rest are kept.

use std::sync::Arc;

use afd_wire::tool_detail::{DETAIL_POST_MAX_BYTES, ToolCallRecordsRequest, ToolCallRecordsStored};
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::response::{IntoResponse as _, Response};

use crate::auth::RunnerIdentity;
use crate::handler::{malformed, refuse, reject};
use crate::services::{Leasing as _, Services};

/// The scoped event a failed post is logged under.
const EVENT: &str = "runner_tool_calls_failed";

/// The refusal a body this daemon cannot read earns.
const DETAIL_MALFORMED: &str = "Malformed tool-call records body";

/// The refusal a body over the post cap earns.
const DETAIL_TOO_LARGE: &str = "A tool-call records post is at most 262144 bytes";

/// Keeps each finished call's full arguments and output.
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = afd_wire::paths::LEASE_TOOL_CALLS,
    tag = afd_http::openapi::tag::RUNNERS,
    operation_id = "runner_record_tool_calls",
    summary = "Save full tool call output",
    description = concat!(
        "Saves completed tool calls before the final run report. Reposting a call replaces ",
        "its record safely. Replaced work assignments are refused without saving anything.\n\n",
        "The body limit is 262144 bytes. Each call's arguments and output may each use ",
        "65536 bytes; an event holds 1 MiB total. Records exceeding these limits count ",
        "toward `skipped_count`; other records are saved.",
    ),
    request_body = ToolCallRecordsRequest,
    params(
        afd_http::openapi::path::Lease,
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = ToolCallRecordsStored),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 409, description = afd_http::openapi::CONFLICT),
        (status = 413, description = afd_http::openapi::PAYLOAD_TOO_LARGE),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn handle<D: Services>(
    State(services): State<Arc<D>>,
    RunnerIdentity(runner): RunnerIdentity,
    Path(lease_id): Path<String>,
    body: Bytes,
) -> Response {
    // Parsed first, so the lease lookup's `::uuid` cast never decides the
    // answer: a value that cannot name any lease is the caller's mistake.
    if let Err(refusal) = super::schedule::lease(&lease_id) {
        return refusal.into_response();
    }
    if body.len() > DETAIL_POST_MAX_BYTES {
        return reject(afd_core::error_code::PAYLOAD_TOO_LARGE, DETAIL_TOO_LARGE);
    }
    // Borrowed out of `body`: every record's output goes straight into a column.
    let Ok(request) = afd_http::handler::read_body::<ToolCallRecordsRequest<'_>>(&body) else {
        return malformed(DETAIL_MALFORMED);
    };
    match services
        .leases()
        .record_tool_calls(runner.id(), &lease_id, &request, services.now())
        .await
    {
        Ok(stored) => Json::<ToolCallRecordsStored>(stored).into_response(),
        Err(error) => refuse(&error, EVENT),
    }
}

#[cfg(test)]
mod tests {
    use afd_wire::tool_detail::DETAIL_POST_MAX_BYTES;

    use super::DETAIL_TOO_LARGE;

    /// The refusal states the cap the handler enforces.
    #[test]
    fn the_size_refusal_names_the_cap_it_enforces() {
        assert!(DETAIL_TOO_LARGE.contains(&DETAIL_POST_MAX_BYTES.to_string()));
    }
}
