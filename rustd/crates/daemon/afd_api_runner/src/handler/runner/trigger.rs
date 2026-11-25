//! `/v1/runners/me/runs/{run_id}/triggers` — a running fleet adding, listing
//! and removing what may wake it.
//!
//! The fleet is the run's: every verb proves the run first and acts on the
//! fleet that run runs, so a body cannot reach another fleet's triggers. An add
//! carries its fence and its call's item in the body and lands once per call,
//! as a schedule create does; a read and a removal carry the fence in the
//! query. The trigger an answer carries is the tenant surface's own, so a
//! person and a fleet read one shape.

use std::sync::Arc;

use af_core::id::Uuid7;
use af_wire::trigger_verb::TriggerAddRequest;
use afd_api_wire::fleet::TriggersResponse;
use afd_fleet::run::Standing;
use afd_fleet::run::effect::Effect;
use afd_fleet::run::write_fence::WriteFence;
use afd_http::handler::{Refusal, parse_id, read_strict_body};
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, RawQuery, State};
use axum::response::{IntoResponse as _, Response};
use http::StatusCode;

use super::effect;
use super::schedule::{fence, standing, write_fence};
use crate::auth::RunnerIdentity;
use crate::services::{RunPlane as _, RunnerPlane};

/// The scoped event a triggers verb that failed is logged under.
const EVENT_FAILED: &str = "runner_triggers_failed";

/// The refusal a body this verb cannot read earns.
const DETAIL_MALFORMED: &str = "Malformed triggers body";

/// The refusal a trigger path segment that is not an identifier earns.
const DETAIL_TRIGGER_ID: &str = "trigger_id must be a valid UUIDv7";

/// Adds a trigger to the fleet the run runs.
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = af_wire::paths::RUN_TRIGGERS,
    tag = afd_http::openapi::tag::FLEETS,
    operation_id = "runner_add_trigger",
    summary = "Add a trigger to the running fleet",
    description = concat!(
        "Adds a webhook, a GitHub App subscription or a Slack mention that wakes the assigned ",
        "fleet. A webhook's `url` is its delivery address. A provider the workspace has not ",
        "connected answers `UZ-AGT-023`. A signing secret its vault lacks answers `UZ-AGT-025`. ",
        "A second trigger of one kind and source answers `UZ-AGT-024`. ",
        "Each call adds one trigger. A request repeating its `item_id` and `call_number` ",
        "returns the first request's trigger and adds nothing.",
    ),
    request_body = TriggerAddRequest<'_>,
    params(afd_http::openapi::path::Run),
    responses(
        (status = 201, description = "The trigger", body = afd_api_wire::fleet::Trigger<'_>),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 409, description = afd_http::openapi::CONFLICT),
        (status = 413, description = afd_http::openapi::PAYLOAD_TOO_LARGE),
        afd_http::openapi::GuardedFailures,
    ),
))]
pub(crate) async fn add<D: RunnerPlane>(
    State(services): State<Arc<D>>,
    RunnerIdentity(runner): RunnerIdentity,
    Path(run_id): Path<String>,
    body: Bytes,
) -> Result<Response, Refusal> {
    let request: TriggerAddRequest<'_> =
        read_strict_body(&body).map_err(|_unreadable| Refusal::malformed(DETAIL_MALFORMED))?;
    let run = standing(&*services, runner.id(), &run_id, request.fencing_token).await?;
    let guard = write_fence(runner.id(), &run_id, request.fencing_token)?;
    let item_id = effect::item_id(&request.item_id)?;
    let item = Effect {
        item_id: &item_id,
        call_number: request.call_number,
        fencing_token: request.fencing_token,
        tool: effect::TRIGGER_ADD,
    };
    let claimed = match effect::claimed(&*services, &run, item, StatusCode::CREATED).await? {
        Ok(claimed) => claimed,
        Err(replayed) => return Ok(replayed),
    };
    let answered = add_once(&*services, &run, &item_id, &request, &guard).await;
    effect::landed(claimed, &request, answered).await
}

/// Writes the trigger `request` names as the row `item_id`; a retry whose
/// write committed answers the row it wrote.
async fn add_once<D: RunnerPlane>(
    services: &D,
    run: &Standing,
    item_id: &Uuid7,
    request: &TriggerAddRequest<'_>,
    guard: &WriteFence,
) -> Result<Response, Refusal> {
    let row = services
        .runs()
        .add_trigger(run, item_id, request, guard)
        .await
        .map_err(Refusal::at(EVENT_FAILED))?;
    let shown = row.shown(services.deployment(), &run.fleet_id);
    Ok((StatusCode::CREATED, Json(shown)).into_response())
}

/// Lists the triggers of the fleet the run runs.
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = af_wire::paths::RUN_TRIGGERS,
    tag = afd_http::openapi::tag::FLEETS,
    operation_id = "runner_list_triggers",
    summary = "List the running fleet's triggers",
    description = "Lists the assigned fleet's triggers, oldest first.",
    params(afd_http::openapi::path::Run, afd_http::openapi::query::Fence),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = TriggersResponse<'_>),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 409, description = afd_http::openapi::CONFLICT),
        afd_http::openapi::GuardedFailures,
    ),
))]
pub(crate) async fn list<D: RunnerPlane>(
    State(services): State<Arc<D>>,
    RunnerIdentity(runner): RunnerIdentity,
    Path(run_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Result<Response, Refusal> {
    let token = fence(query.as_deref())?;
    let run = standing(&*services, runner.id(), &run_id, token).await?;
    let rows = services
        .runs()
        .triggers(&run)
        .await
        .map_err(Refusal::at(EVENT_FAILED))?;
    let deployment = services.deployment();
    Ok(Json(TriggersResponse {
        items: rows
            .iter()
            .map(|row| row.shown(deployment, &run.fleet_id))
            .collect(),
    })
    .into_response())
}

/// Removes one trigger of the fleet the run runs.
#[cfg_attr(feature = "openapi", utoipa::path(
    delete,
    path = af_wire::paths::RUN_TRIGGER,
    tag = afd_http::openapi::tag::FLEETS,
    operation_id = "runner_remove_trigger",
    summary = "Remove one of the running fleet's triggers",
    description = concat!(
        "Removes one trigger of the assigned fleet; deliveries to it are refused from then ",
        "on. A trigger already gone answers 204 as well.",
    ),
    params(
        afd_http::openapi::path::Run,
        ("trigger_id" = String, Path, description = "The trigger's identifier."),
        afd_http::openapi::query::Fence,
    ),
    responses(
        (status = 204, description = afd_http::openapi::NO_CONTENT),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 409, description = afd_http::openapi::CONFLICT),
        afd_http::openapi::GuardedFailures,
    ),
))]
pub(crate) async fn remove<D: RunnerPlane>(
    State(services): State<Arc<D>>,
    RunnerIdentity(runner): RunnerIdentity,
    Path((run_id, trigger_id)): Path<(String, String)>,
    RawQuery(query): RawQuery,
) -> Result<Response, Refusal> {
    let trigger = parse_id(&trigger_id, DETAIL_TRIGGER_ID)?;
    let token = fence(query.as_deref())?;
    let run = standing(&*services, runner.id(), &run_id, token).await?;
    let guard = write_fence(runner.id(), &run_id, token)?;
    // Removed or already gone, the row is absent afterwards: both answer 204.
    services
        .runs()
        .remove_trigger(&run, &trigger, &guard)
        .await
        .map_err(Refusal::at(EVENT_FAILED))?;
    Ok(StatusCode::NO_CONTENT.into_response())
}
