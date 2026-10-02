//! `GET …/events/{event_id}/tool-calls/{call_id}` — one tool call, in full.
//!
//! The read behind "show all" under a call in the thread. The thread carries
//! the call's id as `{fence}:{n}`, and this splits it into the two halves the
//! record is keyed by. An id of any other shape names no record, so it
//! answers exactly as an unknown call does: 404 `TOOL_CALL_NOT_FOUND`, which
//! is also the answer for a call of another workspace's fleet.

use std::borrow::Cow;
use std::sync::Arc;

use afd_core::error_code;
use afd_events::{CallAddress, ToolCallRow};
use afd_wire::tool_detail::ToolCallDetail;
use afd_wire::tool_trace::{fenced_call_id, parse_fenced_call_id};
use axum::Json;
use axum::extract::{Path, State};
use axum::response::{IntoResponse as _, Response};
use serde::Deserialize;

use super::{DETAIL_EVENT_ID, EVENT_ID_MAX_LEN, parse_fleet};
use crate::auth::WorkspaceContext;
use crate::handler::Refusal;
use crate::services::{Services, WorkspaceEvents as _};

/// The scoped event a failed read is logged under.
const EVENT_TOOL_CALL: &str = "fleet_tool_call_detail_failed";

/// The refusal a call this workspace, fleet and event do not hold earns.
const DETAIL_TOOL_CALL_NOT_FOUND: &str = "Tool call not found";

/// The segments the read's template carries.
#[derive(Debug, Deserialize)]
pub(crate) struct ToolCallPath {
    /// The fleet named in the path, still text.
    pub fleet_id: String,
    /// The event named in the path.
    pub event_id: String,
    /// The call, as the thread names it.
    pub call_id: String,
}

/// The fence and call number a `{fence}:{n}` id names, if it names one.
fn parse_call_id(call_id: &str) -> Option<CallAddress> {
    parse_fenced_call_id(call_id).map(|(fence, call_number)| CallAddress { fence, call_number })
}

/// The kept record, as the read answers it.
fn detail(call: CallAddress, row: &ToolCallRow) -> ToolCallDetail<'_> {
    ToolCallDetail {
        call_id: Cow::Owned(fenced_call_id(call.fence, &call.call_number.to_string())),
        // The column is a JSONB object, so its text always parses; the
        // default is the parser's signature, not a reachable answer.
        arguments: serde_json::from_str(&row.arguments).unwrap_or_default(),
        truncated_arguments: row.truncated_arguments,
        output: Cow::Borrowed(&row.output),
        output_line_count: u64::try_from(row.output_line_count).unwrap_or_default(),
        truncated: row.truncated,
    }
}

/// Reads one tool call's full arguments and output.
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/v1/workspaces/{workspace_id}/fleets/{fleet_id}/events/{event_id}/tool-calls/{call_id}",
    tag = afd_http::openapi::tag::FLEETS,
    operation_id = "get_fleet_event_tool_call",
    summary = "Read one tool call in full",
    description = concat!(
        "Returns everything one tool call took and returned, with secret ",
        "values masked. The event's `tool_calls` shows each call's first and ",
        "last lines; read this for the rest. Use the `call_id` the event ",
        "gives, percent-encoded. Arguments and output are each at most 65536 ",
        "bytes, and `truncated_arguments` and `truncated` say when the runner ",
        "cut one to fit. A call whose full output was not kept answers 404, ",
        "as an unknown call and a call in another workspace do. ",
    ),
    params(
        afd_http::openapi::path::ToolCall,
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = ToolCallDetail),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn read<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    Path(ToolCallPath {
        fleet_id,
        event_id,
        call_id,
    }): Path<ToolCallPath>,
) -> Result<Response, Refusal> {
    let fleet = parse_fleet(&fleet_id)?;
    if event_id.is_empty() || event_id.len() > EVENT_ID_MAX_LEN {
        return Err(Refusal::malformed(DETAIL_EVENT_ID));
    }
    let not_found = || Refusal::coded(error_code::TOOL_CALL_NOT_FOUND, DETAIL_TOOL_CALL_NOT_FOUND);
    let call = parse_call_id(&call_id).ok_or_else(not_found)?;
    let found = services
        .events()
        .tool_call(&owned.workspace, &fleet, &event_id, call)
        .await
        .map_err(Refusal::at(EVENT_TOOL_CALL))?;
    let row = found.ok_or_else(not_found)?;
    Ok(Json(detail(call, &row)).into_response())
}

#[cfg(test)]
#[path = "tool_call/tests.rs"]
mod tests;
