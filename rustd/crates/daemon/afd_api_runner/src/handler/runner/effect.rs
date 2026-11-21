//! What a message, schedule write or run-now shares: it lands once per call.
//!
//! The request names its call's item. A handler proves the run first, so a
//! superseded holder is refused before any item is read; then [`claimed`]
//! answers the item's stored answer, or the hold the effect runs under. Once
//! the effect answers, [`landed`] writes a success onto the item before the
//! runner reads it, so a retry answers the same body and nothing happens
//! twice. A refusal writes no item, and the call's retry runs it again.

use af_core::error_code;
use af_core::id::Uuid7;
use afd_fleet::run::Standing;
use afd_fleet::run::effect::{Claim, Claimed, Effect};
use afd_http::handler::{Refusal, parse_id};
use axum::body::{Body, to_bytes};
use axum::response::{IntoResponse as _, Response};
use http::{HeaderValue, StatusCode, header};

use crate::services::{RunPlane as _, RunnerPlane};

/// The tool a message's item names.
pub(super) const MESSAGE: &str = "message";

/// The tool a schedule create's item names.
pub(super) const SCHEDULE_CREATE: &str = "schedule_create";

/// The tool a schedule edit's item names.
pub(super) const SCHEDULE_UPDATE: &str = "schedule_update";

/// The tool a run-now's item names.
pub(super) const SCHEDULE_RUN: &str = "schedule_run";

/// The scoped event a claim or a record that failed is logged under.
const EVENT_FAILED: &str = "runner_effect_failed";

/// The media type a replayed answer carries, as the first one did.
const JSON: &str = "application/json";

/// The refusal an answer whose body could not be read back earns.
const DETAIL_BODY: &str = "The effect's answer could not be read";

/// The refusal an item id that is not one earns.
const DETAIL_ITEM_ID: &str = "item_id must be a valid UUIDv7";

/// The item a request names, proved an identifier.
pub(super) fn item_id(raw: &str) -> Result<Uuid7, Refusal> {
    parse_id(raw, DETAIL_ITEM_ID)
}

/// The hold `effect` runs under, or, for a call that already landed, the
/// answer it stored, replayed with `status`.
pub(super) async fn claimed<D: RunnerPlane>(
    services: &D,
    standing: &Standing,
    effect: Effect<'_>,
    status: StatusCode,
) -> Result<Result<Claimed, Response>, Refusal> {
    let claim = services
        .runs()
        .claim_effect(standing, effect)
        .await
        .map_err(Refusal::at(EVENT_FAILED))?;
    Ok(match claim {
        Claim::Fresh(claimed) => Ok(claimed),
        Claim::Done(stored) => Err((
            status,
            [(header::CONTENT_TYPE, HeaderValue::from_static(JSON))],
            stored,
        )
            .into_response()),
    })
}

/// `answered`, written onto the call's item with what the effect did when it
/// succeeded, then sent; a refusal drops the hold and writes nothing.
pub(super) async fn landed<A: serde::Serialize + Sync>(
    claimed: Claimed,
    arguments: &A,
    answered: Result<Response, Refusal>,
) -> Result<Response, Refusal> {
    let response = answered?;
    if !response.status().is_success() {
        return Ok(response);
    }
    let (parts, body) = response.into_parts();
    let bytes = to_bytes(body, usize::MAX).await.map_err(|_unreadable| {
        Refusal::coded(error_code::INTERNAL_OPERATION_FAILED, DETAIL_BODY)
    })?;
    claimed
        .record(arguments, &String::from_utf8_lossy(&bytes))
        .await
        .map_err(Refusal::at(EVENT_FAILED))?;
    Ok(Response::from_parts(parts, Body::from(bytes)))
}
