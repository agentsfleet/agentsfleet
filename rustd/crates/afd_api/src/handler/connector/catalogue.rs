//! `GET /v1/workspaces/{workspace_id}/connectors` — what this workspace can
//! connect to, and what it already has.
//!
//! The dashboard renders its connector cards from this and from no hard-coded
//! list, which is the whole reason the endpoint exists: a connector shipped in
//! `afd_connector`'s registry appears here without a front-end release, and one
//! withdrawn disappears the same way.
//!
//! # `configured` and `connected` are different facts and stay apart
//!
//! One is about the DEPLOYMENT — has an operator vaulted the `<provider>-app`
//! bag — and the other about this workspace. A single "available" flag would
//! collapse two states a person acts on differently: nothing to press versus a
//! button that starts a connect. It would also hide the real combination where
//! a workspace holds a live grant whose app bag was later removed, which still
//! works for the fleets spending it and can no longer be reconnected.

use std::sync::Arc;

use afd_connector::Catalogued;
use axum::Json;
use axum::extract::State;
use axum::response::{IntoResponse as _, Response};
use serde::Serialize;

use super::EVENT_READ;
use crate::auth::WorkspaceContext;
use crate::handler::Refusal;
use crate::services::{Services, WorkspaceConnectors as _};

/// The wire spelling of a connector whose flow is a consent hop.
///
/// `registry.zig` renders `@tagName(spec.archetype)`, so these two strings are
/// its variant names and are a wire contract the dashboard switches on rather
/// than a description this surface is free to improve.
const ARCHETYPE_OAUTH2: &str = "oauth2";

/// The wire spelling of a connector whose flow is an App installation.
const ARCHETYPE_APP_INSTALL: &str = "app_install";

/// One catalogue row, as this surface renders it.
///
/// No secret material and no field that could carry any — the whole document
/// is four facts about availability.
#[derive(Debug, Serialize)]
struct Entry {
    /// The provider's route segment, which is also its stored id.
    id: &'static str,
    /// Which flow connecting it runs — see [`ARCHETYPE_OAUTH2`].
    archetype: &'static str,
    /// The name a card shows.
    display_name: &'static str,
    /// Whether this DEPLOYMENT has been set up to connect it.
    configured: bool,
    /// Whether THIS workspace holds a landed grant for it.
    connected: bool,
}

impl Entry {
    /// One catalogue row, rendered.
    fn of(row: Catalogued) -> Self {
        Self {
            id: row.provider.id(),
            archetype: if row.is_app_install() {
                ARCHETYPE_APP_INSTALL
            } else {
                ARCHETYPE_OAUTH2
            },
            display_name: row.provider.display_name(),
            configured: row.configured,
            connected: row.connected,
        }
    }
}

/// `GET …/connectors`.
///
/// A bare array rather than an envelope, matching `catalog.zig`: the list is
/// the registry's own length — five today — so there is nothing to page and no
/// cursor for an envelope to carry.
///
/// # Errors
/// Reports a datastore that would not answer.
pub(crate) async fn list<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
) -> Result<Response, Refusal> {
    let catalogue = services
        .connectors()
        .catalogue(services.platform_admin_workspace(), &owned.workspace)
        .await
        .map_err(Refusal::at(EVENT_READ))?;

    let entries: Vec<Entry> = catalogue.into_iter().map(Entry::of).collect();
    Ok(Json(entries).into_response())
}
