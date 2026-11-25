//! `POST /v1/runners/me/runs/{run_id}/access` — a running fleet asking a
//! person for one secret or integration.
//!
//! The fleet is the run's, proved from the path and the body's fence; nothing
//! in the body names a fleet. A request the fleet's approved grant already
//! covers answers 200 at once. Any other sets the grant pending and raises the
//! card a person answers, carrying the run's turn, and answers 201 with the
//! card; the run then ends waiting, as a question's does. A card already open
//! for the name answers 200 and raises none beside it.

use std::borrow::Cow;
use std::sync::Arc;

use af_observability::{AccessChange, FleetStep, Telemetry};
use af_wire::access_verb::{AccessRequest, AccessRequested};
use afd_approval::Requested;
use afd_fleet::run::access::AccessAsked;
use afd_http::handler::{Refusal, read_strict_body};
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::response::{IntoResponse as _, Response};
use http::StatusCode;

use super::schedule::run;
use crate::auth::RunnerIdentity;
use crate::services::{RunPlane as _, RunnerPlane};

/// The scoped event a refused or failed request is logged under.
const EVENT_FAILED: &str = "runner_access_failed";

/// The refusal a body this verb cannot read earns.
const DETAIL_MALFORMED: &str = "Malformed access body";

/// The refusal a name outside its bound earns.
const DETAIL_NAME: &str = "name must be a non-empty vault key or integration with no NUL character";

/// Asks a person for one secret or integration on behalf of the run.
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = af_wire::paths::RUN_ACCESS,
    tag = afd_http::openapi::tag::INTEGRATION_GRANTS,
    operation_id = "runner_request_access",
    summary = "Ask a person for a secret or an integration",
    description = concat!(
        "Asks a person to approve `name` for the assigned fleet: a workspace secret or a ",
        "connected integration. `github` names its `scope`. A request the fleet's approved ",
        "grant already covers answers 200 with no card. Any other raises a card carrying ",
        "the run's turn and answers 201; the run then ends waiting for the person's answer. ",
        "A card already open for the name answers 200. A name the workspace lacks answers ",
        "`UZ-AGT-022`, an integration it has not connected `UZ-AGT-023`.",
    ),
    request_body = AccessRequest<'_>,
    params(afd_http::openapi::path::Run),
    responses(
        (status = 201, description = "A card was raised", body = AccessRequested<'_>),
        (status = 200, description = "Covered, or a card is already open", body = AccessRequested<'_>),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 409, description = afd_http::openapi::CONFLICT),
        (status = 413, description = afd_http::openapi::PAYLOAD_TOO_LARGE),
        afd_http::openapi::GuardedFailures,
    ),
))]
pub(crate) async fn request<D: RunnerPlane>(
    State(services): State<Arc<D>>,
    RunnerIdentity(runner): RunnerIdentity,
    Path(run_id): Path<String>,
    body: Bytes,
) -> Result<Response, Refusal> {
    let request: AccessRequest<'_> =
        read_strict_body(&body).map_err(|_unreadable| Refusal::malformed(DETAIL_MALFORMED))?;
    let request = garde::Unvalidated::new(request)
        .validate()
        .map_err(|_invalid| Refusal::malformed(DETAIL_NAME))?;
    let asked = services
        .runs()
        .request_access(runner.id(), run(&run_id)?, &request, services.now())
        .await
        .map_err(Refusal::at(EVENT_FAILED))?;
    report(&*services, &asked);
    let (status, body) = answered(&asked.requested);
    Ok((status, Json(body)).into_response())
}

/// Reports a raised card as the fleet's request, after its commit; a covered
/// or already-pending request changed nothing.
fn report<D: RunnerPlane>(services: &D, asked: &AccessAsked) {
    if matches!(asked.requested, Requested::Raised { .. }) {
        services.analytics().report(&Telemetry::Fleet {
            actor: None,
            step: FleetStep::AccessChanged {
                fleet_id: asked.fleet.clone(),
                kind: asked.kind.as_str(),
                change: AccessChange::Requested,
            },
        });
    }
}

/// 201 with the card when one was raised; 200 otherwise.
fn answered(requested: &Requested) -> (StatusCode, AccessRequested<'static>) {
    let (status, gate) = match requested {
        Requested::Raised { gate, .. } => (StatusCode::CREATED, Some(gate)),
        Requested::Pending { gate, .. } => (StatusCode::OK, gate.as_ref()),
        Requested::Covered { .. } => (StatusCode::OK, None),
    };
    (
        status,
        AccessRequested {
            grant_id: Cow::Owned(requested.grant().to_string()),
            gate_id: gate.map(|gate| Cow::Owned(gate.to_string())),
        },
    )
}

#[cfg(test)]
mod tests {
    use af_core::id::Uuid7;
    use afd_approval::Requested;
    use http::StatusCode;

    use super::answered;

    #[test]
    fn a_raised_card_is_created_and_a_covered_or_open_one_is_not()
    -> Result<(), af_core::error::Error> {
        let grant = Uuid7::parse("019a0000-0000-7000-8000-0000000000a1")?;
        let gate = Uuid7::parse("019a0000-0000-7000-8000-0000000000b1")?;

        let (status, body) = answered(&Requested::Raised {
            grant: grant.clone(),
            gate: gate.clone(),
        });
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(body.grant_id, grant.as_str());
        assert_eq!(body.gate_id.as_deref(), Some(gate.as_str()));

        let (status, body) = answered(&Requested::Pending {
            grant: grant.clone(),
            gate: Some(gate.clone()),
        });
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body.gate_id.as_deref(), Some(gate.as_str()));

        let (status, body) = answered(&Requested::Covered { grant });
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body.gate_id, None);
        Ok(())
    }
}
