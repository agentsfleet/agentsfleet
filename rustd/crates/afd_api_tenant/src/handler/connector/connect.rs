//! `POST /v1/workspaces/{workspace_id}/connectors/{provider}/connect` — sending
//! a person to consent.
//!
//! Mints this round-trip's single-use nonce, signs a state binding it to the
//! workspace and to whoever pressed the button, and answers where the browser
//! should go. Nothing is stored under the provider's name and no token exists
//! yet: the round-trip finishes at [`super::callback`].
//!
//! # Both archetypes answer one field
//!
//! A consent screen and an App installation page are different destinations and
//! the same instruction — send the browser here — so `install_url` carries
//! either. `connect.zig` splits into `connectOauth2` and `connectAppInstall`
//! and both end at the same `hx.ok(.ok, .{ .install_url = url })`; the split
//! there is about building the URL, which is `afd_connector`'s registry job
//! here and not this handler's.

use std::borrow::Cow;
use std::sync::Arc;

use afd_connector::{Started, Starting};
use afd_wire::connector::ConsentRedirect;
use axum::Json;
use axum::extract::{Path, State};
use axum::response::{IntoResponse as _, Response};

use super::{EVENT_WRITE, provider_of, relay_uri, state_secret, unconfigured};
use crate::auth::{PersonIdentity, WorkspaceContext};
use crate::handler::Refusal;
use crate::services::{Services, WorkspaceConnectors as _};

/// `POST …/connectors/{provider}/connect`.
///
/// # Errors
/// `UZ-CONN-004` for a provider this daemon does not ship, `UZ-CONN-001` for
/// one this deployment has configured no app for, and the store refusals a
/// nonce mint can raise.
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/v1/workspaces/{workspace_id}/connectors/{provider}/connect",
    tag = afd_http::openapi::tag::CONNECTORS,
    operation_id = "connector_connect",
    summary = "Get a provider connection URL",
    description = concat!(
        "Returns a URL for connecting the workspace to the provider. Open the ",
        "URL in a browser. An unknown provider returns 404 `UZ-CONN-004`. An ",
        "unavailable provider returns 503 `UZ-CONN-001`. ",
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 500, description = afd_http::openapi::INTERNAL),
    ),
))]
pub(crate) async fn start<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    person: PersonIdentity,
    Path((_workspace, provider_segment)): Path<(String, String)>,
) -> Result<Response, Refusal> {
    let provider = provider_of(&provider_segment)?;
    // Before the secret read and before the nonce: a deployment with no admin
    // workspace can configure no app, so there is nothing for the rest of this
    // to be doing. It is also the cheapest of the three refusals.
    let Some(admin) = services.platform_admin_workspace() else {
        return Err(unconfigured());
    };
    let secret = state_secret(&services).await?;
    let redirect_uri = relay_uri(&services, provider)?;

    let started = services
        .connectors()
        .start(
            Starting {
                admin,
                workspace: &owned.workspace,
                provider,
                subject: person.subject(),
                redirect_uri: &redirect_uri,
                secret: &secret,
            },
            services.now(),
        )
        .await
        .map_err(Refusal::at(EVENT_WRITE))?;

    match started {
        Started::Consent(install_url) => Ok(Json(ConsentRedirect {
            install_url: Cow::Owned(install_url),
        })
        .into_response()),
        Started::NotConfigured => Err(unconfigured()),
    }
}
