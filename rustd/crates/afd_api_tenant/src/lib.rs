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
use route::{AuthRoute, ConnectorRoute, FleetRoute, TenantRoute, WorkspaceRoute};
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
        TenantRoute::Provider => Some(
            get(handler::tenant::provider_view::<D>)
                .put(handler::tenant::provider_apply::<D>)
                .delete(handler::tenant::provider_reset::<D>),
        ),
        TenantRoute::ModelEntries | TenantRoute::ModelEntry => None,
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
        // No PUT beside the PATCH: a schedule is edited field by field, and a
        // whole-row replace would let a caller silently drop the upstream
        // handle the sync reconciles against.
        FleetRoute::Schedules => {
            Some(get(handler::schedule::list::<D>).post(handler::schedule::create::<D>))
        }
        FleetRoute::Schedule => Some(
            get(handler::schedule::one::<D>)
                .patch(handler::schedule::patch::<D>)
                .delete(handler::schedule::purge::<D>),
        ),
        FleetRoute::ScheduleSync => Some(post(handler::schedule::sync::<D>)),
    }
}

/// Selects the handler for a connector route the caller proves with a bearer.
///
/// `Events` is `None` here on purpose: it is the one route in this family
/// proven by a signature over its body rather than by a credential of ours,
/// and it belongs to the ingress plane. Splitting the family this way is what
/// keeps "every handler in that crate is signature-walled" a fact about a
/// compilation unit rather than a per-route claim.
///
/// # Two routes on one template, and why the guards do not merge
///
/// [`ConnectorRoute::Callback`] and [`ConnectorRoute::Complete`] share
/// `/v1/connectors/{provider}/callback` and differ in GUARD — the provider's
/// redirect carries no credential of ours, the dashboard's completion carries
/// a bearer. The router layers each route with its OWN metadata before merging
/// the two method routers, which is the only reason a same-template pair may
/// disagree about its guard at all.
pub fn connector_handler_for<D: Services>(verb: ConnectorRoute) -> Option<MethodRouter<Arc<D>>> {
    match verb {
        ConnectorRoute::Catalog => Some(get(handler::connector::catalogue::list::<D>)),
        // GET reads and DELETE forgets, on one template: two verbs on one
        // resource, and there is no PUT beside them because a connection is
        // produced by a consent round-trip and cannot be asserted.
        ConnectorRoute::Status => Some(
            get(handler::connector::status::read::<D>)
                .delete(handler::connector::status::disconnect::<D>),
        ),
        ConnectorRoute::Connect => Some(post(handler::connector::connect::start::<D>)),
        ConnectorRoute::Callback => Some(get(handler::connector::callback::relay::<D>)),
        ConnectorRoute::Complete => Some(post(handler::connector::callback::complete::<D>)),
        ConnectorRoute::Events => None,
    }
}
