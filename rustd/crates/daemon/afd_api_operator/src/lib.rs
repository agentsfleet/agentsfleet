//! Operator-facing HTTP adapters.
//!
//! Runner inventory, run history, event projections, and runner mutations
//! live here so operator views compile independently of the host protocol.

#![deny(unused_crate_dependencies)]
#![deny(clippy::self_named_module_files)]

use afd_http::{auth, request_id, route};

pub(crate) mod handler;
mod plane;

pub use self::plane::OperatorPlane;

/// Native response vocabulary shared by served HTTP assertions.
#[cfg(feature = "test-util")]
pub mod test_util {
    /// Refusal for a catalogue page size outside the native bound.
    pub const MODEL_PAGE_LIMIT_DETAIL: &str = crate::handler::admin::models::DETAIL_LIMIT;
}

// The document generator, compiled only when it is asked for.
#[cfg(feature = "openapi")]
pub mod openapi;

use std::sync::Arc;

use axum::routing::{MethodRouter, delete, get, patch, post, put};
use route::{AdminRoute, RunnerOpsRoute};

/// Selects the runner-management handler for a platform or tenant runner route.
pub fn runner_ops_handler_for<D: OperatorPlane>(
    verb: RunnerOpsRoute,
) -> Option<MethodRouter<Arc<D>>> {
    match verb {
        RunnerOpsRoute::Register => Some(post(handler::operator::runner_register::platform::<D>)),
        RunnerOpsRoute::List => Some(get(handler::operator::runners::list::<D>)),
        RunnerOpsRoute::Get => Some(get(handler::operator::runners::detail::<D>)),
        RunnerOpsRoute::Patch => Some(patch(handler::operator::runner_patch::handle::<D>)),
        RunnerOpsRoute::Delete => Some(delete(handler::operator::runner_delete::handle::<D>)),
        RunnerOpsRoute::History => Some(get(handler::operator::events::list::<D>)),
        RunnerOpsRoute::Runs => Some(get(handler::operator::runs::list::<D>)),
        RunnerOpsRoute::TenantRegister => {
            Some(post(handler::operator::tenant_runners::register::<D>))
        }
        RunnerOpsRoute::TenantList => Some(get(handler::operator::tenant_runners::list::<D>)),
        RunnerOpsRoute::TenantGet => Some(get(handler::operator::tenant_runners::detail::<D>)),
        RunnerOpsRoute::TenantPatch => Some(patch(handler::operator::tenant_runners::update::<D>)),
        RunnerOpsRoute::TenantDelete => {
            Some(delete(handler::operator::tenant_runners::delete::<D>))
        }
    }
}

/// Selects the handler for a platform-administration route.
pub fn admin_handler_for<D: OperatorPlane>(verb: AdminRoute) -> MethodRouter<Arc<D>> {
    match verb {
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
