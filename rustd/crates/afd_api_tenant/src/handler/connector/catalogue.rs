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
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/v1/workspaces/{workspace_id}/connectors",
    tag = afd_http::openapi::tag::CONNECTORS,
    operation_id = "connector_catalog",
    summary = "List connectors for a workspace",
    description = concat!(
        "Returns the providers available to the workspace. Each item shows ",
        "whether the provider is ready and connected. Requires the ",
        "`connector:read` scope. ",
    ),
    params(
        afd_http::openapi::path::Workspace,
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = Vec<CatalogueEntry>),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
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

#[cfg(test)]
mod tests {
    use super::{ARCHETYPE_APP_INSTALL, ARCHETYPE_OAUTH2, Catalogued, entry};
    use afd_connector::Provider;

    /// A row for one provider, in the two states the dashboard acts on.
    const fn row(provider: Provider, configured: bool, connected: bool) -> Catalogued {
        Catalogued {
            provider,
            configured,
            connected,
        }
    }

    /// Every shipped provider renders with its own id and display name.
    ///
    /// The dashboard renders cards from this and no hard-coded list, so a
    /// provider whose entry borrowed another's id would draw one card twice and
    /// send the second one's connect button at the first.
    #[test]
    fn every_provider_renders_under_its_own_identity() {
        for provider in Provider::ALL.iter().copied() {
            let rendered = entry(row(provider, true, false));
            assert_eq!(rendered.id, provider.id());
            assert_eq!(rendered.display_name, provider.display_name());
        }
    }

    /// The archetype is the registry's, spelled as the wire contract.
    ///
    /// Both strings are `registry.zig`'s `@tagName(spec.archetype)` — the
    /// dashboard switches on them to decide which connect flow to start, so a
    /// GitHub row rendered as `oauth2` would start a consent round-trip for a
    /// connector that installs an App instead.
    #[test]
    fn an_app_install_and_an_oauth2_connector_are_told_apart() {
        assert_eq!(
            entry(row(Provider::GitHub, true, true)).archetype,
            ARCHETYPE_APP_INSTALL
        );
        for provider in [
            Provider::Slack,
            Provider::Zoho,
            Provider::Jira,
            Provider::Linear,
        ] {
            assert_eq!(
                entry(row(provider, true, true)).archetype,
                ARCHETYPE_OAUTH2,
                "`{provider}` runs a consent round-trip",
            );
        }
    }

    /// `configured` and `connected` survive as two independent facts.
    ///
    /// The module note's reason, pinned: one is about the deployment and the
    /// other about this workspace. Collapsing them would hide the combination
    /// that actually happens — a workspace holding a live grant whose app bag
    /// was later removed, which still works for the fleets spending it and can
    /// no longer be reconnected.
    #[test]
    fn the_deployment_fact_and_the_workspace_fact_do_not_collapse() {
        for configured in [false, true] {
            for connected in [false, true] {
                let rendered = entry(row(Provider::Slack, configured, connected));
                assert_eq!(rendered.configured, configured);
                assert_eq!(rendered.connected, connected);
            }
        }
    }
}
