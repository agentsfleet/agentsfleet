//! Stop: `PATCH …/fleets/{fleet_id}/turns/{turn_id}` and
//! `PATCH …/drafts/{draft_id}/turns/{turn_id}`, with body
//! `{"status":"cancelled"}`.
//!
//! One decision behind two doors. Each path reaches its thread the way its
//! sibling routes do: a fleet's through the workspace the route proved, a
//! draft's through its creator. Both then run the one Stop statement
//! (`afd_turns::view::stop`) and answer through [`decide`]: an unknown turn
//! is 404, a finished one 409 with its state, and a cancelled one 200 however
//! often it is asked (HTTP-PATCH).

use std::sync::Arc;

use af_core::error_code;
use af_core::id::Uuid7;
use afd_api_wire::turn::TurnPatch;
use afd_http::handler::{Refusal, authoring_refusal};
use afd_turns::view::stop::{Marked, Stop, Stopped};
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::response::{IntoResponse as _, Response};
use serde::Deserialize;

use crate::auth::{PersonIdentity, WorkspaceContext};
use crate::services::{TenantPlane, WorkspaceTurns as _};

use super::{DETAIL_TURN_NOT_FOUND, TurnPath, bounded_path, expanded, parse_fleet};

/// The scoped event a fleet Stop's failure is logged under.
const EVENT_STOP: &str = "fleet_turn_stop_failed";

/// Logged once per Stop that cancelled a turn.
const EVENT_STOPPED: &str = "turn_stopped";

/// The refusal a body other than `{"status":"cancelled"}` earns.
const DETAIL_BODY: &str = r#"The body must be {"status":"cancelled"}"#;

/// The refusal Stop on a finished turn earns.
const DETAIL_FINISHED: &str = "This turn already finished; its answer stands";

/// The refusal a draft segment that is not an identifier earns.
const DETAIL_DRAFT_ID: &str = "draft_id must be a UUIDv7";

/// The thread kinds Stop logs, as `thread_kind`.
const KIND_FLEET: &str = "fleet";
const KIND_DRAFT: &str = "draft";

/// The segments a draft turn's template carries.
#[derive(Debug, Deserialize)]
pub(crate) struct DraftTurnPath {
    /// The draft named in the path, still text.
    draft_id: String,
    /// The turn named in the path, still text.
    turn_id: String,
}

/// `PATCH /v1/workspaces/{workspace_id}/fleets/{fleet_id}/turns/{turn_id}`.
#[cfg_attr(feature = "openapi", utoipa::path(
    patch,
    path = "/v1/workspaces/{workspace_id}/fleets/{fleet_id}/turns/{turn_id}",
    tag = afd_http::openapi::tag::FLEETS,
    operation_id = "stop_fleet_turn",
    summary = "Stop a fleet turn",
    description = concat!(
        "Stops a turn with `{\"status\":\"cancelled\"}`. A queued turn is cancelled and never ",
        "runs. A running turn is cancelled with every turn its run serves. The runner ends the ",
        "run within one renewal interval. Its usage is billed, and it gets no answer. A waiting ",
        "turn's open approval closes. Repeating the call answers the same state. A turn that ",
        "already finished answers 409 and keeps its answer. Requires `fleet:write`.",
    ),
    params(afd_http::openapi::path::Turn),
    request_body = TurnPatch,
    responses(
        (status = 200, description = afd_http::openapi::OK, body = af_wire::turn::TurnDetail<'_>),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 409, description = "The turn already finished"),
        (status = 413, description = afd_http::openapi::PAYLOAD_TOO_LARGE),
        afd_http::openapi::GuardedFailures,
    ),
))]
pub(crate) async fn fleet<D: TenantPlane>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    Path(path): Path<TurnPath>,
    body: Bytes,
) -> Result<Response, Refusal> {
    let fleet = parse_fleet(&path.fleet_id)?;
    let path = bounded_path(path)?;
    read_body(&body)?;
    let turn = turn_id(&path.turn_id)?;
    let stopped = services
        .events()
        .stop(&owned.workspace, &fleet, &turn)
        .await
        .map_err(Refusal::at(EVENT_STOP))?;
    let (_sequence, runs) = decide(stopped, KIND_FLEET, &turn)?;
    afd_turns::view::stop::announce(services.runner_commands(), runs).await;
    let event = services
        .events()
        .one(&owned.workspace, &fleet, &turn)
        .await
        .map_err(Refusal::at(EVENT_STOP))?
        .ok_or_else(not_found)?;
    Ok(Json(expanded(&event)).into_response())
}

/// `PATCH /v1/workspaces/{workspace_id}/drafts/{draft_id}/turns/{turn_id}`.
#[cfg_attr(feature = "openapi", utoipa::path(
    patch,
    path = "/v1/workspaces/{workspace_id}/drafts/{draft_id}/turns/{turn_id}",
    tag = afd_http::openapi::tag::DRAFTS,
    operation_id = "stop_draft_turn",
    summary = "Stop a private setup turn",
    description = concat!(
        "Stops one turn of your private draft with `{\"status\":\"cancelled\"}`. A queued turn ",
        "never runs, and a running turn's setup run ends. Later turns stay queued. Repeating ",
        "the call answers the same state; a finished turn answers 409. Requires `fleet:write`.",
    ),
    params(
        afd_http::openapi::path::Draft,
        ("turn_id" = String, Path, description = "The turn to stop."),
    ),
    request_body = TurnPatch,
    responses(
        (status = 200, description = afd_http::openapi::OK, body = afd_api_wire::authoring::Stopped),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 404, description = "Private draft or turn unavailable"),
        (status = 409, description = "The turn already finished"),
        (status = 413, description = afd_http::openapi::PAYLOAD_TOO_LARGE),
        afd_http::openapi::GuardedFailures,
    ),
))]
pub(crate) async fn draft<D: TenantPlane>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    person: PersonIdentity,
    Path(path): Path<DraftTurnPath>,
    body: Bytes,
) -> Result<Response, Refusal> {
    let draft = afd_http::handler::parse_id(&path.draft_id, DETAIL_DRAFT_ID)?;
    read_body(&body)?;
    let turn = turn_id(&path.turn_id)?;
    let access = afd_setup::Access {
        workspace: &owned.workspace,
        subject: person.subject(),
    };
    let stopped = services
        .authoring()
        .stop_turn(access, &draft, &turn)
        .await
        .map_err(authoring_refusal)?;
    // The draft's service already sent the hints, after its own commit.
    let (sequence, _runs) = decide(stopped, KIND_DRAFT, &turn)?;
    Ok(Json(afd_api_wire::authoring::Stopped {
        sequence: sequence.to_string(),
        state: af_core::turn::status::CANCELLED.to_owned(),
    })
    .into_response())
}

/// The one body Stop takes, or the refusal any other earns.
fn read_body(body: &[u8]) -> Result<TurnPatch, Refusal> {
    afd_http::handler::read_body::<TurnPatch>(body)
        .map_err(|_unreadable| Refusal::malformed(DETAIL_BODY))
}

/// The turn the path names; a segment that is not an identifier names no turn.
fn turn_id(text: &str) -> Result<Uuid7, Refusal> {
    Uuid7::parse(text).map_err(|_text| not_found())
}

fn not_found() -> Refusal {
    Refusal::coded(error_code::TURN_NOT_FOUND, DETAIL_TURN_NOT_FOUND)
}

/// What one Stop answers: the turn's sequence, with the runs owed a Cancel
/// hint when it cancelled the turn and none when the turn was already
/// cancelled; a refusal otherwise. Both doors answer through this, so the two
/// cannot disagree.
fn decide(
    stopped: Option<Stopped>,
    thread_kind: &str,
    turn: &Uuid7,
) -> Result<(i64, Vec<Marked>), Refusal> {
    let Some(Stopped { sequence, stop }) = stopped else {
        return Err(not_found());
    };
    match stop {
        Stop::Finished(status) => Err(Refusal::conflict(
            error_code::TURN_FINISHED,
            DETAIL_FINISHED,
            &status,
        )),
        Stop::AlreadyCancelled => Ok((sequence, Vec::new())),
        Stop::Cancelled {
            running,
            runs,
            turns,
            approvals,
        } => {
            let turn_id = turn.as_str();
            let event = EVENT_STOPPED;
            tracing::info!(event, thread_kind, turn_id, running, turns, approvals);
            Ok((sequence, runs))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{decide, read_body};
    use af_core::id::Uuid7;
    use afd_turns::view::stop::{Marked, Stop, Stopped};
    use axum::response::IntoResponse as _;
    use http::StatusCode;

    type Outcome = Result<(), Box<dyn std::error::Error>>;

    fn turn() -> Result<Uuid7, af_core::error::Error> {
        Uuid7::parse("0195b4ba-8d3a-7f13-8abc-2b3e1e0bb001")
    }

    fn stopped(stop: Stop) -> Stopped {
        Stopped { sequence: 4, stop }
    }

    /// The status a refused decision answers with, or none for a Stop.
    fn refused(stopped: Option<Stopped>, turn: &Uuid7) -> Option<StatusCode> {
        decide(stopped, "fleet", turn)
            .err()
            .map(|refusal| refusal.into_response().status())
    }

    #[test]
    fn only_the_cancelled_status_is_a_stop() {
        let refused = |body: &[u8]| {
            read_body(body)
                .err()
                .map(|refusal| refusal.into_response().status())
        };
        assert_eq!(refused(br#"{"status":"cancelled"}"#), None);
        for body in [
            &br#"{"status":"answered"}"#[..],
            br#"{"event_id":"x","status":"cancelled"}"#,
            b"{}",
            b"",
        ] {
            assert_eq!(refused(body), Some(StatusCode::BAD_REQUEST));
        }
    }

    #[test]
    fn each_decision_answers_its_own_way() -> Outcome {
        let turn = turn()?;
        let marked = Marked {
            runner: turn.clone(),
            run: turn.clone(),
        };
        let cancelled = Stop::Cancelled {
            running: true,
            runs: vec![marked.clone()],
            turns: 1,
            approvals: 0,
        };
        assert_eq!(
            decide(Some(stopped(cancelled)), "fleet", &turn).ok(),
            Some((4, vec![marked]))
        );
        assert_eq!(
            decide(Some(stopped(Stop::AlreadyCancelled)), "fleet", &turn).ok(),
            Some((4, Vec::new()))
        );
        let finished = stopped(Stop::Finished("processed".to_owned()));
        assert_eq!(refused(Some(finished), &turn), Some(StatusCode::CONFLICT));
        assert_eq!(refused(None, &turn), Some(StatusCode::NOT_FOUND));
        Ok(())
    }
}
