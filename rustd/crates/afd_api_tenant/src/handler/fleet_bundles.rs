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
