//! A person's first message makes the fleet, its thread and its first turn.

use super::{access, input, render};
use crate::{
    auth::{PersonIdentity, WorkspaceContext},
    services::TenantPlane,
};
use af_observability::{FleetOrigin, FleetStep, Telemetry};
use afd_auth::principal::PersonCredential;
use afd_http::handler::{Refusal, setup_refusal};
use axum::{body::Bytes, extract::State, response::Response};
use std::sync::Arc;

#[cfg_attr(feature = "openapi", utoipa::path(
    post, path = "/v1/workspaces/{workspace_id}/fleets", operation_id = "create_fleet",
    tag = afd_http::openapi::tag::FLEETS, summary = "Start a fleet with its first message",
    description = concat!(
        "Creates an active fleet, its thread and its first turn in one transaction. The ",
        "fleet runs on `model_ref`, or on the workspace's selected model when it is omitted. ",
        "It is named `name`, or `fleet-` and the last 12 hex digits of its id. Its answer ",
        "arrives on `GET /v1/workspaces/{workspace_id}/fleets/{fleet_id}/turns/stream`. ",
        "Retry with the same `operation_id` and body to receive the same fleet with ",
        "`replayed: true`. The same key with another body answers 409 `UZ-SETUP-005`. ",
        "A key whose fleet was deleted answers 410 `UZ-SETUP-013`.",
        " Invalid images return 400 `UZ-AGT-026`. A text-only model returns 409 `UZ-AGT-027`.",
    ),
    params(afd_http::openapi::path::Workspace),
    request_body = afd_api_wire::fleet::CreateFleetRequest,
    responses((status = 201, body = afd_api_wire::fleet::CreatedFleet<'_>, description = "The new fleet and its first turn", headers(("Location" = String), ("ETag" = String))),
        (status = 200, body = afd_api_wire::fleet::CreatedFleet<'_>, description = "A repeat of an earlier create: the same fleet and turn", headers(("Location" = String), ("ETag" = String))),
        (status = 400, description = "Invalid message, key, model or name"),
        (status = 409, description = "No usable model (`UZ-SETUP-008`), a taken name (`UZ-AGT-006`), or a key used with another body (`UZ-SETUP-005`)"),
        (status = 410, description = "The key's fleet was deleted (`UZ-SETUP-013`)"),
        (status = 413, description = "Body too large"),
        afd_http::openapi::GuardedFailures,
    )
))]
pub(crate) async fn create<D: TenantPlane>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    person: PersonIdentity,
    body: Bytes,
) -> Result<Response, Refusal> {
    let request = input::create(&body)?;
    crate::handler::fleet::image_input::validate(&request.message, &request.images).await?;
    let created = services
        .creations()
        .create(access(&owned, &person), &request, services.now())
        .await
        .map_err(setup_refusal)?;
    if !created.replayed {
        services.analytics().report(&Telemetry::Fleet {
            actor: Some(person.subject().to_owned()),
            step: FleetStep::Created {
                fleet_id: created.fleet.id.clone(),
                origin: origin(person.person().credential()),
            },
        });
    }
    render::created(services.as_ref(), &owned.workspace, created).await
}

/// Where a first message came from, by the class of the credential that sent
/// it: a browser session is the dashboard, anything else a terminal.
const fn origin(credential: &PersonCredential) -> FleetOrigin {
    match credential {
        PersonCredential::SessionToken { .. } => FleetOrigin::Dashboard,
        PersonCredential::CliCredential | PersonCredential::TenantApiKey => FleetOrigin::Cli,
    }
}

#[cfg(test)]
mod tests {
    use af_observability::FleetOrigin;
    use afd_auth::principal::PersonCredential;

    use super::origin;

    #[test]
    fn a_browser_session_is_the_dashboard_and_any_other_credential_a_terminal() {
        assert_eq!(
            origin(&PersonCredential::SessionToken {
                workspace_scope: None
            }),
            FleetOrigin::Dashboard
        );
        assert_eq!(origin(&PersonCredential::CliCredential), FleetOrigin::Cli);
        assert_eq!(origin(&PersonCredential::TenantApiKey), FleetOrigin::Cli);
    }
}
