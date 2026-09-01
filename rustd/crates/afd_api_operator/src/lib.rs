//! Operator-facing HTTP adapters.
//!
//! Runner inventory, lease history, event projections, and runner mutations
//! live here so operator views compile independently of the host protocol.

pub use afd_http::{admission, auth, envelope, etag, request_id, route, services};

pub(crate) mod handler;

// The document generator, compiled only when it is asked for.
#[cfg(feature = "openapi")]
pub mod openapi;

use std::sync::Arc;

use axum::routing::{MethodRouter, delete, get, patch, post, put};
use route::{AdminRoute, RunnerOpsRoute};
use services::Services;

/// Selects an operator handler; runner registration belongs to the runner crate.
pub fn runner_ops_handler_for<D: Services>(verb: RunnerOpsRoute) -> Option<MethodRouter<Arc<D>>> {
    match verb {
        RunnerOpsRoute::Register => None,
        RunnerOpsRoute::List => Some(get(handler::operator::runners::list::<D>)),
        RunnerOpsRoute::Get => Some(get(handler::operator::runners::detail::<D>)),
        RunnerOpsRoute::Patch => Some(patch(handler::operator::runner_patch::handle::<D>)),
        RunnerOpsRoute::Events => Some(get(handler::operator::events::list::<D>)),
        RunnerOpsRoute::Leases => Some(get(handler::operator::leases::list::<D>)),
    }
}

/// Selects the handler for a platform-administration route.
pub fn admin_handler_for<D: Services>(verb: AdminRoute) -> MethodRouter<Arc<D>> {
    match verb {
        AdminRoute::FleetLibrary => get(handler::admin::libraries::list::<D>)
            .merge(post(handler::admin::library_import::create::<D>)),
        AdminRoute::FleetLibraryEntry => patch(handler::admin::libraries::patch::<D>)
            .merge(delete(handler::admin::libraries::delete::<D>)),
        AdminRoute::PlatformKeys => get(handler::admin::platform_keys::list::<D>)
            .merge(put(handler::admin::platform_keys::set::<D>)),
        AdminRoute::PlatformKey => delete(handler::admin::platform_keys::deactivate::<D>),
        AdminRoute::Models => {
            get(handler::admin::models::list::<D>).merge(post(handler::admin::models::create::<D>))
        }
        AdminRoute::Model => patch(handler::admin::models::update::<D>)
            .merge(delete(handler::admin::models::delete::<D>)),
    }
}
