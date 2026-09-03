//! Public metadata gallery for published Fleet Bundles.

use std::borrow::Cow;
use std::sync::Arc;

use afd_library::PublicLibraryItem;
use afd_wire::admin::{FleetBundleItem, FleetBundlesResponse};
use axum::Json;
use axum::extract::State;
use axum::response::{IntoResponse as _, Response};

use crate::handler::refuse;
use crate::services::Services;

/// Lists published platform entries carrying current bundle content.
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/v1/fleets/bundles",
    tag = afd_http::openapi::tag::FLEET_BUNDLES,
    operation_id = "list_fleet_bundles",
    summary = "List the platform Fleet library catalog",
    description = concat!(
        "Returns the first-party (platform) Fleet library catalog. Rows ",
        "contain only metadata and requirement hints; they carry no ",
        "credential values or object-store keys. The workspace gallery union ",
        "lives at GET /v1/workspaces/{workspace_id}/fleet-libraries. ",
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = FleetBundlesResponse),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn list<D: Services>(State(services): State<Arc<D>>) -> Response {
    match services.libraries().published().await {
        Ok(entries) => Json(FleetBundlesResponse {
            items: entries.iter().map(item).collect(),
        })
        .into_response(),
        Err(error) => refuse(&error, "fleet_bundles_list_failed"),
    }
}

fn item(entry: &PublicLibraryItem) -> FleetBundleItem<'static> {
    FleetBundleItem {
        id: Cow::Owned(entry.id().to_owned()),
        name: Cow::Owned(entry.name().to_owned()),
        description: Cow::Owned(entry.description().to_owned()),
        required_credentials: entry
            .required_credentials()
            .iter()
            .cloned()
            .map(Cow::Owned)
            .collect(),
        required_credentials_reasons: entry.required_credentials_reasons().clone(),
        required_tools: entry
            .required_tools()
            .iter()
            .cloned()
            .map(Cow::Owned)
            .collect(),
        network_hosts: entry
            .network_hosts()
            .iter()
            .cloned()
            .map(Cow::Owned)
            .collect(),
    }
}
