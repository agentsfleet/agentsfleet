//! Native publications stay inside the authenticated runner's current run.

use crate::{auth::RunnerIdentity, services::RunnerPlane};
use af_wire::durable_task::{
    DurableTaskEffectInput, DurableTaskEffectQuery, DurableTaskEffectState, DurableTaskEffectsQuery,
};
use afd_fleet::durable_task::DurableTaskRun;
use afd_http::handler::{Refusal, parse_id, read_strict_body};
use afd_http::{envelope::ProblemResponse, request_id::RequestId};
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
    post, path = af_wire::paths::RUN_DURABLE_TASK_EFFECTS, tag = afd_http::openapi::tag::RUNNERS,
    operation_id = "runner_publish_durable_task_effect", summary = "Publish one tested exact-head repair",
    description = "Records immutable expected-head and tested-tree authority before one native publication. Refuses workflow changes, unsupported file modes, stale ownership and unresolved effects. Unknown acknowledgements require reconciliation and never repeat the mutation.",
    params(("run_id" = String, Path, description = "Current run UUIDv7.")), request_body = DurableTaskEffectInput,
    responses((status = 201, body = af_wire::durable_task::DurableTaskEffect, description = "Confirmed new publication"),
        (status = 200, body = af_wire::durable_task::DurableTaskEffect, description = "Saved operation replay"),
        (status = 400, description = afd_http::openapi::BAD_REQUEST), (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 409, description = "Coded problem with the immutable effect receipt; UZ-DT-010 identifies an uncertain publication"),
        (status = 413, description = afd_http::openapi::PAYLOAD_TOO_LARGE),
        afd_http::openapi::GuardedFailures,
    )
))]
pub(crate) async fn publish<D: RunnerPlane>(
    State(services): State<Arc<D>>,
    RunnerIdentity(runner): RunnerIdentity,
    Path(run): Path<String>,
    body: Bytes,
) -> Result<Response, Refusal> {
    let run_id = parse_id(&run, "run_id must be a UUIDv7")?;
    let input: DurableTaskEffectInput =
        read_strict_body(&body).map_err(|_invalid| Refusal::malformed(MALFORMED))?;
    let input = garde::Unvalidated::new(input)
        .validate()
        .map_err(|_invalid| Refusal::malformed(MALFORMED))?;
    let run = DurableTaskRun {
        runner_id: runner.id(),
        run_id: &run_id,
        fencing_token: input.fencing_token,
    };
    let (effect, replayed) = services
        .tasks()
        .publish_effect(run, &input)
        .await
        .map_err(Refusal::at(EVENT))?;
    if effect.state != DurableTaskEffectState::Confirmed {
        let reason = if matches!(
            effect.state,
            DurableTaskEffectState::Issued | DurableTaskEffectState::Unknown
        ) {
            afd_fleet::durable_task::DurableTaskRefusal::EffectUncertain
        } else {
            afd_fleet::durable_task::DurableTaskRefusal::StaleState
        };
        let mut response = ProblemResponse::effect_conflict(
            reason.code(),
            reason.detail(),
            RequestId::current(),
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
    get, path = af_wire::paths::RUN_DURABLE_TASK_EFFECT, tag = afd_http::openapi::tag::RUNNERS,
    operation_id = "runner_read_durable_task_effect", summary = "Read and reconcile one publication",
    description = "Returns the immutable publication identity and verified receipt. Reconciles unknown outcomes through bounded exact-branch history reads; absence never authorizes another mutation.",
    params(("run_id" = String, Path, description = "Current run UUIDv7."),
        ("effect_id" = String, Path, description = "Native effect UUIDv7."), DurableTaskEffectQuery),
    responses((status = 200, body = af_wire::durable_task::DurableTaskEffect, description = "Native publication state"),
        (status = 400, description = afd_http::openapi::BAD_REQUEST), (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 409, description = afd_http::openapi::CONFLICT),
        afd_http::openapi::GuardedFailures,
    )
))]
pub(crate) async fn read<D: RunnerPlane>(
    State(services): State<Arc<D>>,
    RunnerIdentity(runner): RunnerIdentity,
    Path((run, effect)): Path<(String, String)>,
    input: Result<Query<DurableTaskEffectQuery>, QueryRejection>,
) -> Result<Response, Refusal> {
    let run_id = parse_id(&run, "run_id must be a UUIDv7")?;
    let effect_id = parse_id(&effect, "effect_id must be a UUIDv7")?;
    let Query(input) = input.map_err(|_invalid| Refusal::malformed(MALFORMED))?;
    let input = garde::Unvalidated::new(input)
        .validate()
        .map_err(|_invalid| Refusal::malformed(MALFORMED))?;
    let run = DurableTaskRun {
        runner_id: runner.id(),
        run_id: &run_id,
        fencing_token: input.fencing_token,
    };
    let effect = services
        .tasks()
        .effect(run, &effect_id)
        .await
        .map_err(Refusal::at(EVENT))?;
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(effect)).into_response())
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get, path = af_wire::paths::RUN_DURABLE_TASK_EFFECTS, tag = afd_http::openapi::tag::RUNNERS,
    operation_id = "runner_list_durable_task_effects", summary = "Recover publications after a checkpoint watermark",
    description = "Pages native task-local effect order, including unresolved and newly confirmed effects. A replacement worker needs no previously returned effect ID.",
    params(("run_id" = String, Path, description = "Current run UUIDv7."), DurableTaskEffectsQuery),
    responses((status = 200, body = af_wire::durable_task::DurableTaskEffectsPage, description = "Ordered recoverable publication states"),
        (status = 400, description = afd_http::openapi::BAD_REQUEST), (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 409, description = afd_http::openapi::CONFLICT),
        afd_http::openapi::GuardedFailures,
    )
))]
pub(crate) async fn list<D: RunnerPlane>(
    State(services): State<Arc<D>>,
    RunnerIdentity(runner): RunnerIdentity,
    Path(run): Path<String>,
    input: Result<Query<DurableTaskEffectsQuery>, QueryRejection>,
) -> Result<Response, Refusal> {
    let run_id = parse_id(&run, "run_id must be a UUIDv7")?;
    let Query(input) = input.map_err(|_invalid| Refusal::malformed(MALFORMED))?;
    let input = garde::Unvalidated::new(input)
        .validate()
        .map_err(|_invalid| Refusal::malformed(MALFORMED))?;
    let run = DurableTaskRun {
        runner_id: runner.id(),
        run_id: &run_id,
        fencing_token: input.fencing_token,
    };
    let page = services
        .tasks()
        .effects(run, &input)
        .await
        .map_err(Refusal::at(EVENT))?;
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(page)).into_response())
}
