//! Route-family composition over independently compiled API planes.
//!
//! The plane crates own handler selection within their route families. This
//! module remains the sole composition root: it decides which plane owns each
//! top-level [`Route`] and which tabled families are intentionally unserved.

use std::sync::Arc;

use axum::routing::{MethodRouter, get};

use crate::route::{Route, RunnerOpsRoute};

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
        Route::Auth(verb) => afd_api_tenant::auth_handler_for::<D>(verb),
        Route::Tenant(verb) => afd_api_tenant::tenant_handler_for::<D>(verb),
        Route::Runner(verb) => Some(afd_api_runner::handler_for::<D>(verb)),
        Route::RunnerOps(RunnerOpsRoute::Register) => {
            Some(afd_api_runner::enrolment_handler::<D>())
        }
        Route::RunnerOps(verb) => afd_api_operator::runner_ops_handler_for::<D>(verb),
        Route::Workspace(verb) => afd_api_tenant::workspace_handler_for::<D>(verb),
        Route::Fleet(verb) => afd_api_tenant::fleet_handler_for::<D>(verb),
        Route::Admin(verb) => Some(afd_api_operator::admin_handler_for::<D>(verb)),
        Route::Webhook(_) | Route::Connector(_) => None,
    }
}
