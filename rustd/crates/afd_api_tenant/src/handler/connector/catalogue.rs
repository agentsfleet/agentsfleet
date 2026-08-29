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

use std::borrow::Cow;
use std::sync::Arc;

use afd_connector::Catalogued;
use afd_wire::connector::{ARCHETYPE_APP_INSTALL, ARCHETYPE_OAUTH2, CatalogueEntry};
use axum::Json;
use axum::extract::State;
use axum::response::{IntoResponse as _, Response};

use super::EVENT_READ;
use crate::auth::WorkspaceContext;
use crate::handler::Refusal;
use crate::services::{Services, WorkspaceConnectors as _};

/// One catalogue row, as the wire renders it.
///
/// The shape and its two archetype spellings are `afd_wire::connector`'s. Both
/// strings are `registry.zig`'s `@tagName(spec.archetype)`, so they are a wire
/// contract the dashboard switches on rather than a description this surface
/// is free to improve.
fn entry(row: Catalogued) -> CatalogueEntry<'static> {
    CatalogueEntry {
        id: Cow::Borrowed(row.provider.id()),
        archetype: Cow::Borrowed(if row.is_app_install() {
            ARCHETYPE_APP_INSTALL
        } else {
            ARCHETYPE_OAUTH2
        }),
        display_name: Cow::Borrowed(row.provider.display_name()),
        configured: row.configured,
        connected: row.connected,
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

    let entries: Vec<CatalogueEntry<'_>> = catalogue.into_iter().map(entry).collect();
    Ok(Json(entries).into_response())
}
