//! `GET|DELETE /v1/workspaces/{workspace_id}/connectors/{provider}` — what one
//! connection is, and letting it go.
//!
//! Two methods on one template because they are two verbs on one resource: the
//! connection either is there or is not, and both answers are about the same
//! thing. There is no PUT beside them — a connection is created by the consent
//! round-trip and cannot be asserted by writing it.
//!
//! # A disconnect revokes nothing at the provider
//!
//! It removes this daemon's sealed handle and the rows routing the provider's
//! events back, and leaves the authorization standing at the vendor. That is
//! what makes reconnecting always available after any drift — see
//! [`afd_connector::Grants::forget`], and `disconnect.zig` for the same rule
//! stated from the other side.

use std::sync::Arc;

use afd_connector::{Connection, Forgotten};
use axum::Json;
use axum::extract::{Path, State};
use axum::response::{IntoResponse as _, Response};
use http::StatusCode;
use serde::Serialize;

use super::{EVENT_READ, EVENT_WRITE, provider_of};
use crate::auth::WorkspaceContext;
use crate::handler::Refusal;
use crate::services::{Services, WorkspaceConnectors as _};

/// What a workspace holding a landed grant is told.
///
/// `oauth_status.zig`'s `STATUS_CONNECTED`, kept byte-for-byte: the dashboard
/// switches on this string and a cutover has both daemons answering the route.
const STATUS_CONNECTED: &str = "connected";

/// What a workspace holding nothing is told — see [`STATUS_CONNECTED`].
const STATUS_NOT_CONNECTED: &str = "not_connected";

/// One provider's connection, as this surface renders it.
///
/// Carries no token and no expiry. A status read answers whether a person has
/// connected and what it is called, and every other field of the stored handle
/// is the broker's business.
#[derive(Debug, Serialize)]
struct View<'v> {
    /// Whether this workspace holds a landed grant.
    status: &'static str,
    /// What a person sees the connection called, when the grant named one.
    ///
    /// Always present in the document and `null` when absent, rather than
    /// omitted: a dashboard reading `label` on an object that sometimes lacks
    /// the key would have to branch on undefined as well as on null.
    label: Option<&'v str>,
}

impl<'v> View<'v> {
    /// One connection, or the absence of one, rendered.
    fn of(connection: Option<&'v Connection>) -> Self {
        connection.map_or(
            Self {
                status: STATUS_NOT_CONNECTED,
                label: None,
            },
            |connection| Self {
                status: STATUS_CONNECTED,
                label: connection.label.as_deref(),
            },
        )
    }
}

/// `GET …/connectors/{provider}`.
///
/// Never fabricates a connected state: every shape that is not a landed grant
/// reads as not connected — see [`afd_connector::Grants::connection`].
///
/// # Errors
/// `UZ-CONN-004` for a provider this daemon does not ship, and a datastore that
/// would not answer.
pub(crate) async fn read<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    Path((_workspace, provider_segment)): Path<(String, String)>,
) -> Result<Response, Refusal> {
    let provider = provider_of(&provider_segment)?;
    let connection = services
        .connectors()
        .connection(&owned.workspace, provider)
        .await
        .map_err(Refusal::at(EVENT_READ))?;

    Ok(Json(View::of(connection.as_ref())).into_response())
}

/// `DELETE …/connectors/{provider}`.
///
/// 204 whether or not anything was held. Idempotent in the way a delete is
/// asked to be: the caller wanted the connection gone, and it is gone —
/// answering 404 for a second press would make a person believe their first
/// one had failed.
///
/// # Errors
/// `UZ-CONN-004` for a provider this daemon does not ship, a datastore that
/// would not answer, and a vault that refused the delete.
pub(crate) async fn disconnect<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    Path((_workspace, provider_segment)): Path<(String, String)>,
) -> Result<Response, Refusal> {
    let provider = provider_of(&provider_segment)?;
    let forgotten = services
        .connectors()
        .forget(&owned.workspace, provider)
        .await
        .map_err(Refusal::at(EVENT_WRITE))?;

    // Both outcomes answer 204, and the value is still worth matching on: a
    // reader here can see that the two were considered and deliberately
    // collapsed, where a discarded return would leave that a guess.
    match forgotten {
        Forgotten::Disconnected | Forgotten::AlreadyAbsent => {
            Ok(StatusCode::NO_CONTENT.into_response())
        }
    }
}
