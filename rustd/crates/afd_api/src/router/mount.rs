//! Route-family composition over independently compiled API planes.
//!
//! The plane crates own handler selection within their route families. This
//! module remains the sole composition root: it decides which plane owns each
//! top-level [`Route`] and which tabled families are intentionally unserved.

use std::sync::Arc;

use axum::routing::{MethodRouter, get};

use axum::extract::DefaultBodyLimit;

use crate::route::{ConnectorRoute, Route, RunnerOpsRoute};

use super::{Serving, probes};

/// Returns the handler for `route`, or `None` when this daemon does not serve it.
///
/// The top-level match is total so a new route family cannot silently inherit
/// another plane's authentication or admission policy.
pub(super) fn handler_for<D: Serving>(route: Route) -> Option<MethodRouter<Arc<D>>> {
    match route {
        Route::Ops(verb) => Some(match verb {
            crate::route::OpsRoute::Healthz => get(probes::healthz),
            crate::route::OpsRoute::Readyz => get(probes::readyz::<D>),
        }),
        // Two planes answer this family, exactly as they do for connectors:
        // the bearer-proven half is the tenant's, and the identity provider's
        // signature-proven signup event is the ingress plane's. It carries the
        // webhook family's buffer cap for the same reason that one does — the
        // proof IS the body, so the body must be read before anything can
        // decide whether to trust it.
        Route::Auth(verb) => afd_api_tenant::auth_handler_for::<D>(verb).or_else(|| {
            afd_api_ingress::auth_handler_for::<D>(verb)
                .map(|handler| handler.layer(DefaultBodyLimit::max(afd_api_ingress::BUFFER_CEILING)))
        }),
        Route::Tenant(verb) => afd_api_tenant::tenant_handler_for::<D>(verb),
        Route::Runner(verb) => Some(afd_api_runner::handler_for::<D>(verb)),
        Route::RunnerOps(RunnerOpsRoute::Register) => {
            Some(afd_api_runner::enrolment_handler::<D>())
        }
        Route::RunnerOps(verb) => afd_api_operator::runner_ops_handler_for::<D>(verb),
        Route::Workspace(verb) => afd_api_tenant::workspace_handler_for::<D>(verb),
        Route::Fleet(verb) => afd_api_tenant::fleet_handler_for::<D>(verb),
        Route::Admin(verb) => Some(afd_api_operator::admin_handler_for::<D>(verb)),
        // Two planes answer this family: the bearer-proven half is the
        // tenant's, the signature-proven events route is the ingress plane's.
        // Asking the tenant first and falling through keeps the split readable
        // as "whoever proves the caller owns the route".
        Route::Connector(verb) => afd_api_tenant::connector_handler_for::<D>(verb).or_else(|| {
            afd_api_ingress::connector_handler_for::<D>(verb).map(|handler| match verb {
                // The one connector route reachable with no credential at all,
                // because the proof IS the body and cannot be checked until the
                // body has been read. It therefore carries the buffer cap the
                // webhook family carries throughout.
                ConnectorRoute::Events => {
                    handler.layer(DefaultBodyLimit::max(afd_api_ingress::BUFFER_CEILING))
                }
                _ => handler,
            })
        }),
        Route::Webhook(verb) => Some(
            afd_api_ingress::webhook_handler_for::<D>(verb)
                .layer(DefaultBodyLimit::max(afd_api_ingress::BUFFER_CEILING)),
        ),
    }
}
