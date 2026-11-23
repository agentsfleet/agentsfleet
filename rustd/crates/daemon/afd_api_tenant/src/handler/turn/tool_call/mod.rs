//! `GET …/turns/{turn_id}/items/{item_id}` — one tool call, in full.
//!
//! The read behind "show all" under a call in the thread. The thread carries
//! the call's id as `{fence}:{n}`, and this splits it into the two halves the
//! record is keyed by. An id of any other shape names no record, so it
//! answers exactly as an unknown call does: 404 `TOOL_CALL_NOT_FOUND`, which
//! is also the answer for a call of another workspace's fleet.

use std::borrow::Cow;
use std::sync::Arc;

use af_core::error_code;
use af_wire::tool_detail::ToolCallDetail;
use af_wire::tool_trace::{fenced_call_id, parse_fenced_call_id};
use afd_turns::view::{CallAddress, ToolCallRow};
use axum::Json;
use axum::extract::{Path, State};
use axum::response::{IntoResponse as _, Response};
use serde::Deserialize;

use super::{TURN_ID_MAX_LEN, bounded_path, parse_fleet};
use crate::auth::WorkspaceContext;
use crate::handler::Refusal;
use crate::services::{TenantPlane, WorkspaceTurns as _};

/// The scoped event a failed read is logged under.
const EVENT_TOOL_CALL: &str = "fleet_turn_item_detail_failed";

/// The refusal a call this workspace, fleet and event do not hold earns.
const DETAIL_TOOL_CALL_NOT_FOUND: &str = "Tool call not found";

/// The segments the read's template carries, each renamed from its segment.
#[derive(Debug, Deserialize, garde::Validate)]
pub(crate) struct ToolCallPath {
    /// The fleet named in the path, still text.
    #[serde(rename = "fleet_id")]
    // Parsed rather than bounded: an identifier has one length.
    #[garde(skip)]
    pub fleet: String,
    /// The turn named in the path.
    #[serde(rename = "turn_id")]
    #[garde(length(bytes, min = 1, max = TURN_ID_MAX_LEN))]
    pub event: String,
    /// The item, as the thread names its call.
    #[serde(rename = "item_id")]
    // Parsed rather than bounded: `{fence}:{n}` is a grammar.
    #[garde(skip)]
    pub call: String,
}

/// The fence and call number a `{fence}:{n}` id names, if it names one.
fn parse_call_id(call_id: &str) -> Option<CallAddress> {
    parse_fenced_call_id(call_id).map(|(fence, call_number)| CallAddress { fence, call_number })
}

/// The kept record, as the read answers it.
fn detail(call: CallAddress, row: &ToolCallRow) -> ToolCallDetail<'_> {
    ToolCallDetail {
        call_id: Cow::Owned(fenced_call_id(call.fence, &call.call_number.to_string())),
        kind: row.kind,
        tool: row.tool.as_deref().map(Cow::Borrowed),
        status: row.status,
        // The column is a JSONB object, so its text always parses; the
        // default is the parser's signature, not a reachable answer.
        arguments: serde_json::from_str(&row.arguments).unwrap_or_default(),
        truncated_arguments: row.truncated_arguments,
        output: Cow::Borrowed(&row.output),
        output_line_count: u64::try_from(row.output_line_count).unwrap_or_default(),
        truncated: row.truncated,
        // The column holds the list the record verb narrowed, so its text
        // always parses; the default is the parser's signature.
        changes: row
            .changes
            .as_deref()
            .and_then(|text| serde_json::from_str(text).ok())
            .unwrap_or_default(),
    }
}

/// Reads one tool call's full arguments and output.
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/v1/workspaces/{workspace_id}/fleets/{fleet_id}/turns/{turn_id}/items/{item_id}",
    tag = afd_http::openapi::tag::FLEETS,
    operation_id = "get_fleet_turn_item",
    summary = "Read one tool call in full",
    description = concat!(
        "Returns full tool arguments and output with secrets masked. URL-encode the `item_id`, ",
        "the trace's `call_id`. Neither is capped; `truncated_arguments` and `truncated` mark ",
        "content the runner shortened to fit one 33554432-byte post. Missing, unrecorded, or ",
        "inaccessible calls return 404.",
    ),
    params(
        afd_http::openapi::path::Item,
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = ToolCallDetail<'_>),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        afd_http::openapi::GuardedFailures,
    ),
))]
pub(crate) async fn read<D: TenantPlane>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    Path(path): Path<ToolCallPath>,
) -> Result<Response, Refusal> {
    let fleet = parse_fleet(&path.fleet)?;
    let path = bounded_path(path)?;
    let not_found = || Refusal::coded(error_code::TOOL_CALL_NOT_FOUND, DETAIL_TOOL_CALL_NOT_FOUND);
    let call = parse_call_id(&path.call).ok_or_else(not_found)?;
    let found = services
        .events()
        .tool_call(
            &owned.workspace,
            &fleet,
            &af_core::id::Uuid7::parse(&path.event).map_err(|_text| not_found())?,
            call,
        )
        .await
        .map_err(Refusal::at(EVENT_TOOL_CALL))?;
    let row = found.ok_or_else(not_found)?;
    Ok(Json(detail(call, &row)).into_response())
}

#[cfg(test)]
mod tests;
