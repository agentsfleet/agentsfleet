//! Conditional controls reach the same native owner as lease-bound task work.

use std::sync::Arc;

use af_validate::Sentences;
use afd_api_wire::fleet::{PatchTaskRequest, TaskControlState};
use afd_fleet::task::{TaskAdoption, TaskControl, TaskScope};
use afd_http::handler::{Refusal, setup_input::expected};
use axum::{
    body::Bytes,
    extract::{Path, State},
    response::Response,
};
use http::HeaderMap;

use super::{
    input::{self, TaskPath},
    read,
};
use crate::{auth::WorkspaceContext, handler::fleet::detail::parse_fleet_id, services::Services};

const EVENT: &str = "fleet_task_control_failed";
const MALFORMED: &str = "Task control exceeds its supported shape or bounds";
const REQUIRED: &str = "Send the current task If-Match tag";
const GENERATION: &str = "Task generation must be positive";
const SIZE: &str = "Task control exceeds 128 KiB";
const SENTENCES: Sentences = Sentences::new(
    &[("expected_generation", GENERATION), ("bytes", SIZE)],
    MALFORMED,
);

#[derive(garde::Validate)]
struct Envelope<'a> {
    #[garde(length(max = 128 * 1024))]
    bytes: &'a [u8],
}

#[cfg_attr(feature = "openapi", utoipa::path(
    patch, path = "/v1/workspaces/{workspace_id}/fleets/{fleet_id}/tasks/{task_id}",
    tag = afd_http::openapi::tag::FLEETS, operation_id = "patch_fleet_task",
    summary = "Control fleet work or adopt reviewed goal criteria",
    description = "Requires FleetWrite and the current strong If-Match tag. Native version and generation checks run while holding the task lock. Controls preserve the accepted goal, cumulative usage and deadline. Ready resumes paused or blocked work within its remaining allowance; cancellation does not claim rollback of issued effects.",
    params(afd_http::openapi::path::Fleet,
        ("task_id" = String, Path, description = "Task UUIDv7."),
        ("If-Match" = String, Header, description = "Required current strong resource tag.")),
    request_body = PatchTaskRequest,
    responses((status = 200, body = afd_api_wire::fleet::TaskResponse<'_>, description = "Current task", headers(("ETag" = String))),
        (status = 400, description = "Invalid task control"), (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"), (status = 404, description = "Task unavailable"),
        (status = 409, description = "Generation or lifecycle refuses mutation"),
        (status = 412, description = "Stale resource tag; problem body carries the current etag"),
        (status = 413, description = "Body too large"), (status = 428, description = "Missing tag"),
        (status = 429, description = "Task allowance or request limit reached"), (status = 500, description = "Internal failure"),
        (status = 503, description = "Dependency unavailable"))
))]
pub(crate) async fn change<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    Path(TaskPath { fleet_id, task_id }): Path<TaskPath>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, Refusal> {
    let scope = TaskScope {
        workspace_id: owned.workspace,
        fleet_id: parse_fleet_id(&fleet_id)?,
    };
    let task = input::task(&task_id)?;
    let envelope = garde::Unvalidated::new(Envelope { bytes: &body })
        .validate()
        .map_err(|report| {
            Refusal::coded(
                af_core::error_code::PAYLOAD_TOO_LARGE,
                SENTENCES.pick(&report),
            )
        })?;
    let request: PatchTaskRequest = af_core::json::object_from_slice(envelope.bytes)
        .map_err(|_invalid| Refusal::malformed(MALFORMED))?;
    let request = garde::Unvalidated::new(request)
        .validate()
        .map_err(|report| Refusal::malformed(SENTENCES.pick(&report)))?;
    let tag = expected(&headers)?
        .ok_or_else(|| Refusal::coded(af_core::error_code::AUTHORING_PRECONDITION, REQUIRED))?;
    let saved = match request.into_inner() {
        PatchTaskRequest::Control {
            state,
            expected_generation,
        } => {
            let control = match state {
                TaskControlState::Paused => TaskControl::Pause,
                TaskControlState::Ready => TaskControl::Resume,
                TaskControlState::Cancelled => TaskControl::Cancel,
            };
            services
                .tasks()
                .control(&scope, &task, tag, expected_generation, control)
                .await
        }
        PatchTaskRequest::AdoptGoal {
            goal_revision_id,
            expected_generation,
            operation_id,
        } => {
            let adoption = TaskAdoption {
                goal_revision_id: afd_http::handler::parse_id(
                    &goal_revision_id,
                    "goal_revision_id must be a UUIDv7",
                )?,
                operation_id: afd_http::handler::parse_id(
                    &operation_id,
                    "operation_id must be a UUIDv7",
                )?,
                expected_generation,
            };
            services
                .tasks()
                .adopt_goal(&scope, &task, tag, &adoption)
                .await
        }
    }
    .map_err(refusal)?;
    Ok(read::render_one(saved))
}

fn refusal(fault: afd_fleet::Error) -> Refusal {
    if let Some(current) = fault.task_precondition_tag() {
        return Refusal::preconditioned(fault.code(), fault.detail(), current);
    }
    if af_core::problem::Problem::of(fault.code()).status() == 409
        && let Some(state) = fault.task_current_state()
    {
        return Refusal::conflict(fault.code(), fault.detail(), state);
    }
    Refusal::at(EVENT)(fault)
}
