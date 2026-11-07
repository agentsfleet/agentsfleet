//! Native publications stay inside the authenticated runner's current lease.

use crate::{auth::RunnerIdentity, services::Services};
use afd_fleet::task::TaskLease;
use afd_http::handler::{Refusal, parse_id, read_strict_body};
use afd_http::{envelope::ProblemResponse, request_id::RequestId};
use afd_wire::task::{TaskEffectInput, TaskEffectQuery, TaskEffectState, TaskEffectsQuery};
use axum::{
    Json,
    body::Bytes,
    extract::{Path, Query, State, rejection::QueryRejection},
    response::{IntoResponse as _, Response},
};
use http::{StatusCode, header};
use std::sync::Arc;

const EVENT: &str = "runner_task_effect_failed";
const MALFORMED: &str = "Publication request exceeds its supported shape or bounds";

#[cfg_attr(feature = "openapi", utoipa::path(
    post, path = afd_wire::paths::LEASE_TASK_EFFECTS, tag = afd_http::openapi::tag::RUNNERS,
    operation_id = "runner_publish_task_effect", summary = "Publish one tested exact-head repair",
    description = "Records immutable expected-head and tested-tree authority before one native publication. Refuses workflow changes, unsupported file modes, stale ownership and unresolved effects. Unknown acknowledgements require reconciliation and never repeat the mutation.",
    params(("lease_id" = String, Path, description = "Current lease UUIDv7.")), request_body = TaskEffectInput,
    responses((status = 201, body = afd_wire::task::TaskEffect, description = "Confirmed new publication"),
        (status = 200, body = afd_wire::task::TaskEffect, description = "Saved operation replay"),
        (status = 400, description = afd_http::openapi::BAD_REQUEST), (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN), (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 409, description = "Coded problem with the immutable effect receipt; UZ-DT-010 identifies an uncertain publication"),
        (status = 413, description = afd_http::openapi::PAYLOAD_TOO_LARGE), (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL), (status = 503, description = afd_http::openapi::UNAVAILABLE))
))]
pub(crate) async fn publish<D: Services>(
    State(services): State<Arc<D>>,
    RunnerIdentity(runner): RunnerIdentity,
    Path(lease): Path<String>,
    body: Bytes,
) -> Result<Response, Refusal> {
    let lease_id = parse_id(&lease, "lease_id must be a UUIDv7")?;
    let input: TaskEffectInput =
        read_strict_body(&body).map_err(|_invalid| Refusal::malformed(MALFORMED))?;
    let input = garde::Unvalidated::new(input)
        .validate()
        .map_err(|_invalid| Refusal::malformed(MALFORMED))?;
    let lease = TaskLease {
        runner_id: runner.id(),
        lease_id: &lease_id,
        fencing_token: input.fencing_token,
    };
    let (effect, replayed) = services
        .tasks()
        .publish_effect(lease, &input)
        .await
        .map_err(Refusal::at(EVENT))?;
    if effect.state != TaskEffectState::Confirmed {
        let reason = if matches!(
            effect.state,
            TaskEffectState::Issued | TaskEffectState::Unknown
        ) {
            afd_fleet::task::TaskRefusal::EffectUncertain
        } else {
            afd_fleet::task::TaskRefusal::StaleState
        };
        let mut response = ProblemResponse::effect_conflict(
            reason.code(),
            reason.detail(),
            RequestId::mint(),
            effect,
        )
        .into_response();
        response.headers_mut().insert(
            header::CACHE_CONTROL,
            http::HeaderValue::from_static("no-store"),
        );
        return Ok(response);
    }
    let status = if replayed {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };
    Ok((status, [(header::CACHE_CONTROL, "no-store")], Json(effect)).into_response())
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get, path = afd_wire::paths::LEASE_TASK_EFFECT, tag = afd_http::openapi::tag::RUNNERS,
    operation_id = "runner_read_task_effect", summary = "Read and reconcile one publication",
    description = "Returns the immutable publication identity and verified receipt. Reconciles unknown outcomes through bounded exact-branch history reads; absence never authorizes another mutation.",
    params(("lease_id" = String, Path, description = "Current lease UUIDv7."),
        ("effect_id" = String, Path, description = "Native effect UUIDv7."), TaskEffectQuery),
    responses((status = 200, body = afd_wire::task::TaskEffect, description = "Native publication state"),
        (status = 400, description = afd_http::openapi::BAD_REQUEST), (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN), (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 409, description = afd_http::openapi::CONFLICT), (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL), (status = 503, description = afd_http::openapi::UNAVAILABLE))
))]
pub(crate) async fn read<D: Services>(
    State(services): State<Arc<D>>,
    RunnerIdentity(runner): RunnerIdentity,
    Path((lease, effect)): Path<(String, String)>,
    input: Result<Query<TaskEffectQuery>, QueryRejection>,
) -> Result<Response, Refusal> {
    let lease_id = parse_id(&lease, "lease_id must be a UUIDv7")?;
    let effect_id = parse_id(&effect, "effect_id must be a UUIDv7")?;
    let Query(input) = input.map_err(|_invalid| Refusal::malformed(MALFORMED))?;
    let input = garde::Unvalidated::new(input)
        .validate()
        .map_err(|_invalid| Refusal::malformed(MALFORMED))?;
    let lease = TaskLease {
        runner_id: runner.id(),
        lease_id: &lease_id,
        fencing_token: input.fencing_token,
    };
    let effect = services
        .tasks()
        .effect(lease, &effect_id)
        .await
        .map_err(Refusal::at(EVENT))?;
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(effect)).into_response())
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get, path = afd_wire::paths::LEASE_TASK_EFFECTS, tag = afd_http::openapi::tag::RUNNERS,
    operation_id = "runner_list_task_effects", summary = "Recover publications after a checkpoint watermark",
    description = "Pages native task-local effect order, including unresolved and newly confirmed effects. A replacement worker needs no previously returned effect ID.",
    params(("lease_id" = String, Path, description = "Current lease UUIDv7."), TaskEffectsQuery),
    responses((status = 200, body = afd_wire::task::TaskEffectsPage, description = "Ordered recoverable publication states"),
        (status = 400, description = afd_http::openapi::BAD_REQUEST), (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN), (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 409, description = afd_http::openapi::CONFLICT), (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL), (status = 503, description = afd_http::openapi::UNAVAILABLE))
))]
pub(crate) async fn list<D: Services>(
    State(services): State<Arc<D>>,
    RunnerIdentity(runner): RunnerIdentity,
    Path(lease): Path<String>,
    input: Result<Query<TaskEffectsQuery>, QueryRejection>,
) -> Result<Response, Refusal> {
    let lease_id = parse_id(&lease, "lease_id must be a UUIDv7")?;
    let Query(input) = input.map_err(|_invalid| Refusal::malformed(MALFORMED))?;
    let input = garde::Unvalidated::new(input)
        .validate()
        .map_err(|_invalid| Refusal::malformed(MALFORMED))?;
    let lease = TaskLease {
        runner_id: runner.id(),
        lease_id: &lease_id,
        fencing_token: input.fencing_token,
    };
    let page = services
        .tasks()
        .effects(lease, &input)
        .await
        .map_err(Refusal::at(EVENT))?;
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(page)).into_response())
}
