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

use std::borrow::Cow;

use afd_connector::{Connection, Forgotten};
use afd_wire::connector::{ConnectionView, STATUS_CONNECTED, STATUS_NOT_CONNECTED};
use axum::Json;
use axum::extract::{Path, State};
use axum::response::{IntoResponse as _, Response};
use http::StatusCode;

use super::{EVENT_READ, EVENT_WRITE, provider_of};
use crate::auth::WorkspaceContext;
use crate::handler::Refusal;
use crate::services::{Services, WorkspaceConnectors as _};

/// One connection, or the absence of one, as the wire renders it.
///
/// The shape and its two status spellings are `afd_wire::connector`'s — see
/// that module on why a response type declared beside its handler is a contract
/// only one side can see.
fn view(connection: Option<&Connection>) -> ConnectionView<'_> {
    connection.map_or(
        ConnectionView {
            status: Cow::Borrowed(STATUS_NOT_CONNECTED),
            label: None,
        },
        |connection| ConnectionView {
            status: Cow::Borrowed(STATUS_CONNECTED),
            label: connection.label.as_deref().map(Cow::Borrowed),
        },
    )
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

    Ok(Json(view(connection.as_ref())).into_response())
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

#[cfg(test)]
mod tests {
    use super::{Connection, STATUS_CONNECTED, STATUS_NOT_CONNECTED, view};

    /// Nothing held reads as not connected, and names nothing.
    ///
    /// The absence is a rendered state rather than a 404: the dashboard draws a
    /// card for every shipped provider, and this is the one that offers a
    /// connect button.
    #[test]
    fn an_absent_connection_reads_as_not_connected() {
        let rendered = view(None);
        assert_eq!(rendered.status, STATUS_NOT_CONNECTED);
        assert_eq!(rendered.label, None);
    }

    /// A held connection reads as connected and carries what it is called.
    #[test]
    fn a_held_connection_carries_the_name_a_person_recognises() {
        let held = Connection {
            label: Some("Acme Jira".to_owned()),
        };
        let rendered = view(Some(&held));
        assert_eq!(rendered.status, STATUS_CONNECTED);
        assert_eq!(rendered.label.as_deref(), Some("Acme Jira"));
    }

    /// A labelless grant is connected, not broken.
    ///
    /// The case worth its own test: a provider whose answer named nothing still
    /// landed a spendable grant. Reading the missing label as "not connected"
    /// would offer a connect button for a connection that already works, and
    /// pressing it would replace a live grant.
    #[test]
    fn a_connection_carrying_no_label_is_still_connected() {
        let unlabelled = Connection { label: None };
        let rendered = view(Some(&unlabelled));
        assert_eq!(rendered.status, STATUS_CONNECTED);
        assert_eq!(rendered.label, None);
    }
}
