//! Conditional controls and a person's statement edit reach the same native
//! owner as run-bound task work. Only a signed-in person edits a task's
//! statements; a control is unchanged.

use std::sync::Arc;

use af_validate::Sentences;
use afd_api_wire::fleet::{DurableTaskControlState, PatchDurableTaskRequest};
use afd_fleet::durable_task::{DurableTaskControl, DurableTaskScope, DurableTaskStatements};
use afd_http::handler::{Refusal, setup_input::expected};
use axum::{
    body::Bytes,
    extract::{Path, State},
    response::Response,
};
use http::HeaderMap;

use super::{
    input::{self, DurableTaskPath},
    read,
};
use crate::{
    auth::{PersonIdentity, SignedInClass, WorkspaceContext},
    handler::fleet::detail::parse_fleet_id,
    services::TenantPlane,
};

const EVENT: &str = "fleet_task_control_failed";
const MALFORMED: &str = "Task control exceeds its supported shape or bounds";
const REQUIRED: &str = "Send the current task If-Match tag";
const GENERATION: &str = "Task generation must be positive";
const STATEMENTS: &str =
    "questions must hold at least one statement, each with a unique id and within its bounds";
const SENTENCES: Sentences = Sentences::new(
    &[
        ("expected_generation", GENERATION),
        ("questions", STATEMENTS),
    ],
    MALFORMED,
);

#[cfg_attr(feature = "openapi", utoipa::path(
    patch, path = "/v1/workspaces/{workspace_id}/fleets/{fleet_id}/durable_tasks/{durable_task_id}",
    tag = afd_http::openapi::tag::FLEETS, operation_id = "patch_fleet_durable_task",
    summary = "Control fleet work or edit its completion statements",
    description = "Requires FleetWrite and the current strong If-Match tag. Native version and generation checks run while holding the durable task lock. Controls preserve the accepted goal, cumulative usage and deadline. Ready resumes paused or blocked work within its remaining allowance; cancellation does not claim rollback of issued effects. A statement edit replaces `questions`, writes a new goal digest and raises the generation by one. It needs a signed-in person; an API key receives 403. A stale generation receives 412 `UZ-DT-013`.",
    params(afd_http::openapi::path::Fleet,
        ("durable_task_id" = String, Path, description = "Durable task UUIDv7."),
        ("If-Match" = String, Header, description = "Required current strong resource tag.")),
    request_body = PatchDurableTaskRequest,
    responses((status = 200, body = afd_api_wire::fleet::DurableTaskResponse<'_>, description = "Current durable task", headers(("ETag" = String))),
        (status = 400, description = "Invalid durable task control"), (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN), (status = 404, description = "Durable task unavailable"),
        (status = 409, description = "Generation or lifecycle refuses mutation"),
        (status = 412, description = "Stale resource tag; problem body carries the current etag"),
        (status = 413, description = "Body too large"), (status = 428, description = "Missing tag"),
        (status = 429, description = "Durable task allowance or request limit reached"), (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE))
))]
pub(crate) async fn change<D: TenantPlane>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    person: PersonIdentity,
    Path(DurableTaskPath {
        fleet_id,
        durable_task_id,
    }): Path<DurableTaskPath>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, Refusal> {
    let scope = DurableTaskScope {
        workspace_id: owned.workspace,
        fleet_id: parse_fleet_id(&fleet_id)?,
    };
    let task = input::task(&durable_task_id)?;
    let request: PatchDurableTaskRequest = af_core::json::object_from_slice(&body)
        .map_err(|_invalid| Refusal::malformed(MALFORMED))?;
    let request = garde::Unvalidated::new(request)
        .validate()
        .map_err(|report| Refusal::malformed(SENTENCES.pick(&report)))?;
    if matches!(*request, PatchDurableTaskRequest::Statements { .. }) {
        person.narrow::<SignedInClass>()?;
    }
    let tag = expected(&headers)?
        .ok_or_else(|| Refusal::coded(af_core::error_code::SETUP_PRECONDITION, REQUIRED))?;
    let saved = match request.into_inner() {
        PatchDurableTaskRequest::Control {
            state,
            expected_generation,
        } => {
            let control = match state {
                DurableTaskControlState::Paused => DurableTaskControl::Pause,
                DurableTaskControlState::Ready => DurableTaskControl::Resume,
                DurableTaskControlState::Cancelled => DurableTaskControl::Cancel,
            };
            services
                .tasks()
                .control(&scope, &task, tag, expected_generation, control)
                .await
        }
        PatchDurableTaskRequest::Statements {
            questions,
            expected_generation,
        } => {
            let edit = DurableTaskStatements {
                questions,
                expected_generation,
            };
            services
                .tasks()
                .edit_statements(&scope, &task, tag, &edit)
                .await
        }
    }
    .map_err(refusal)?;
    Ok(read::render_one(saved))
}

fn refusal(fault: afd_fleet::Error) -> Refusal {
    if let Some(current) = fault.durable_task_precondition_tag() {
        return Refusal::preconditioned(fault.code(), fault.detail(), current);
    }
    if af_core::problem::Problem::of(fault.code()).status() == 409
        && let Some(state) = fault.durable_task_current_state()
    {
        return Refusal::conflict(fault.code(), fault.detail(), state);
    }
    Refusal::at(EVENT)(fault)
}
