//! Tenant-facing HTTP adapters.
//!
//! API-key, command-line credential, workspace, billing, and model handlers
//! live here so changes to tenant surfaces compile independently of runner and
//! operator handlers.

pub use afd_http::{admission, auth, client, envelope, etag, request_id, route, services};

mod handler;
pub use handler::{fleet, secret, tenant};

use std::sync::Arc;

use axum::routing::{MethodRouter, delete, get, patch, post, put};
use route::{AuthRoute, FleetRoute, TenantRoute, WorkspaceRoute};
use services::Services;

/// Selects the device-flow handler for an authentication route.
pub fn auth_handler_for<D: Services>(verb: AuthRoute) -> Option<MethodRouter<Arc<D>>> {
    match verb {
        AuthRoute::CreateSession => Some(post(handler::auth::open::<D>)),
        AuthRoute::PollSession => Some(get(handler::auth::poll::<D>)),
        AuthRoute::ApproveSession => Some(patch(handler::auth::approve::<D>)),
        AuthRoute::VerifySession => Some(post(handler::auth::verify::<D>)),
        AuthRoute::DeleteAllSessions => Some(delete(handler::auth::delete_all::<D>)),
        AuthRoute::DeleteSession => Some(delete(handler::auth::delete_one::<D>)),
        AuthRoute::IdentityEventClerk => None,
    }
}

/// Selects the handler for a tenant-owned route.
pub fn tenant_handler_for<D: Services>(verb: TenantRoute) -> Option<MethodRouter<Arc<D>>> {
    match verb {
        TenantRoute::ApiKeys => {
            Some(get(handler::tenant::list::<D>).post(handler::tenant::mint::<D>))
        }
        TenantRoute::ApiKey => {
            Some(patch(handler::tenant::revoke::<D>).delete(handler::tenant::delete::<D>))
        }
        TenantRoute::CliCredentials => Some(post(handler::tenant::mint_cli::<D>)),
        TenantRoute::CliCredential => Some(delete(handler::tenant::revoke_cli::<D>)),
        TenantRoute::Billing => Some(get(handler::tenant::billing_snapshot::<D>)),
        TenantRoute::BillingCharges => Some(get(handler::tenant::billing_charges::<D>)),
        TenantRoute::Workspaces => Some(get(handler::tenant::list_workspaces::<D>)),
        TenantRoute::CreateWorkspace => Some(post(handler::tenant::create_workspace::<D>)),
        TenantRoute::ModelLibrary => Some(get(handler::tenant::catalogue::<D>)),
        TenantRoute::FleetBundles => Some(get(handler::fleet_bundles::list::<D>)),
        TenantRoute::Provider | TenantRoute::ModelEntries | TenantRoute::ModelEntry => None,
    }
}

/// Selects the handler for a workspace collection or item route.
pub fn workspace_handler_for<D: Services>(verb: WorkspaceRoute) -> Option<MethodRouter<Arc<D>>> {
    match verb {
        WorkspaceRoute::Fleets => {
            Some(get(handler::fleet::list::<D>).post(handler::fleet::install::<D>))
        }
        WorkspaceRoute::Secrets => {
            Some(get(handler::secret::list::<D>).post(handler::secret::store::<D>))
        }
        WorkspaceRoute::Secret => {
            Some(put(handler::secret::replace::<D>).delete(handler::secret::remove::<D>))
        }
        WorkspaceRoute::Onboarding => Some(get(handler::preference::onboarding::<D>)),
        WorkspaceRoute::Preferences => Some(get(handler::preference::read::<D>)),
        WorkspaceRoute::Preference => Some(put(handler::preference::write::<D>)),
        WorkspaceRoute::Approvals => Some(get(handler::approval::list::<D>)),
        WorkspaceRoute::Approval => Some(get(handler::approval::detail::<D>)),
        WorkspaceRoute::ApprovalResolve => Some(post(handler::approval::resolve::<D>)),
        WorkspaceRoute::Events => Some(get(handler::event::workspace_list::<D>)),
        WorkspaceRoute::EventsStream => Some(get(handler::stream::workspace::<D>)),
        WorkspaceRoute::FleetLibrary => None,
    }
}

/// Selects the handler for a route scoped to one fleet.
pub fn fleet_handler_for<D: Services>(verb: FleetRoute) -> Option<MethodRouter<Arc<D>>> {
    match verb {
        FleetRoute::Detail => Some(
            get(handler::fleet::detail::read::<D>)
                .patch(handler::fleet::detail::patch::<D>)
                .delete(handler::fleet::detail::purge::<D>),
        ),
        FleetRoute::Events => Some(get(handler::event::fleet_list::<D>)),
        FleetRoute::Event => Some(get(handler::event::detail::<D>)),
        FleetRoute::Grants => Some(get(handler::grant::list::<D>)),
        FleetRoute::Grant => Some(delete(handler::grant::revoke::<D>)),
        FleetRoute::Memories => Some(get(handler::fleet::memory::list::<D>)),
        FleetRoute::Memory => Some(delete(handler::fleet::memory::forget::<D>)),
        FleetRoute::Messages => Some(
            get(handler::fleet::message::thread::<D>).post(handler::fleet::message::steer::<D>),
        ),
        FleetRoute::EventsStream => Some(get(handler::stream::fleet::<D>)),
        FleetRoute::Schedules | FleetRoute::Schedule | FleetRoute::ScheduleSync => None,
    }
}
