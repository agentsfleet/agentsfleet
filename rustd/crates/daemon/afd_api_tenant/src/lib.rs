//! Tenant-facing HTTP adapters.
//!
//! API-key, command-line credential, workspace, billing, and model handlers
//! live here so changes to tenant surfaces compile independently of runner and
//! operator handlers.

#![deny(unused_crate_dependencies)]
#![deny(clippy::self_named_module_files)]

pub use afd_http::{admission, auth, client, envelope, etag, request_id, route, services};

pub(crate) mod handler;

// The document generator, compiled only when it is asked for.
#[cfg(feature = "openapi")]
pub mod openapi;
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
        AuthRoute::DeleteOwnSessions => Some(delete(handler::auth::delete_all::<D>)),
        AuthRoute::IdentityEventClerk => None,
    }
}

/// Selects the handler for a tenant-owned route.
pub fn tenant_handler_for<D: Services>(verb: TenantRoute) -> Option<MethodRouter<Arc<D>>> {
    match verb {
        TenantRoute::ApiKeys => Some(get(tenant::list::<D>).post(tenant::mint::<D>)),
        TenantRoute::ApiKey => Some(patch(tenant::revoke::<D>).delete(tenant::delete::<D>)),
        TenantRoute::CliCredentials => Some(post(tenant::mint_cli::<D>)),
        TenantRoute::CurrentUser => Some(get(tenant::current_user::<D>)),
        TenantRoute::Invites => {
            Some(get(tenant::invite::list::<D>).post(tenant::invite::create::<D>))
        }
        TenantRoute::Invite => Some(delete(tenant::invite::revoke::<D>)),
        TenantRoute::InviteEmail => Some(post(tenant::invite_email::send::<D>)),
        TenantRoute::Members => Some(get(tenant::member::list::<D>)),
        TenantRoute::Member => Some(delete(tenant::member::remove::<D>)),
        TenantRoute::InvitesForMe => Some(get(tenant::invite::waiting::<D>)),
        TenantRoute::InviteAcceptance => Some(post(tenant::invite::accept::<D>)),
        TenantRoute::CliCredential => Some(delete(tenant::revoke_cli::<D>)),
        TenantRoute::Billing => Some(get(tenant::billing_snapshot::<D>)),
        TenantRoute::BillingCharges => Some(get(tenant::billing_charges::<D>)),
        TenantRoute::BillingReport => Some(get(tenant::billing_report::report::<D>)),
        TenantRoute::BillingActivityCharges => Some(get(tenant::billing_report::charges::<D>)),
        TenantRoute::Workspaces => Some(get(tenant::list_workspaces::<D>)),
        TenantRoute::CreateWorkspace => Some(post(tenant::create_workspace::<D>)),
        TenantRoute::ModelLibrary => Some(get(tenant::catalogue::<D>)),
        TenantRoute::Provider => Some(
            get(tenant::provider_view::<D>)
                .put(tenant::provider_apply::<D>)
                .delete(tenant::provider_reset::<D>),
        ),
        TenantRoute::ModelEntries => {
            Some(get(tenant::list_model_entries::<D>).post(tenant::create_model_entry::<D>))
        }
        TenantRoute::ModelEntry => {
            Some(patch(tenant::update_model_entry::<D>).delete(tenant::remove_model_entry::<D>))
        }
    }
}

/// Selects the handler for a workspace collection or item route.
pub fn workspace_handler_for<D: Services>(verb: WorkspaceRoute) -> Option<MethodRouter<Arc<D>>> {
    match verb {
        WorkspaceRoute::Models
        | WorkspaceRoute::Drafts
        | WorkspaceRoute::Draft
        | WorkspaceRoute::DraftMessages
        | WorkspaceRoute::DraftStream
        | WorkspaceRoute::DraftCancellations => {
            draft_handler_for::<D>(verb).map(handler::draft::private)
        }
        WorkspaceRoute::Fleets => {
            Some(get(fleet::list::<D>).post(fleet::native::create::save::<D>))
        }
        WorkspaceRoute::Secrets => Some(get(secret::list::<D>).post(secret::store::<D>)),
        WorkspaceRoute::Secret => Some(put(secret::replace::<D>).delete(secret::remove::<D>)),
        WorkspaceRoute::Onboarding => Some(get(handler::preference::onboarding::<D>)),
        WorkspaceRoute::Preference => Some(put(handler::preference::write::<D>)),
        WorkspaceRoute::Approvals => Some(get(handler::approval::list::<D>)),
        WorkspaceRoute::Approval => Some(get(handler::approval::detail::<D>)),
        WorkspaceRoute::ApprovalResolve => Some(post(handler::approval::resolve::resolve::<D>)),
        WorkspaceRoute::Events => Some(get(handler::event::workspace_list::<D>)),
        WorkspaceRoute::EventsStream => Some(get(handler::stream::workspace::<D>)),
        WorkspaceRoute::FleetLibrary => Some(
            get(handler::library::list::gallery::<D>).post(handler::library::publish::create::<D>),
        ),
        WorkspaceRoute::FleetLibraryOwned => Some(get(handler::library::list::owned::<D>)),
        WorkspaceRoute::FleetLibraryItem => {
            Some(get(handler::library::read::one::<D>).delete(handler::library::remove::one::<D>))
        }
        WorkspaceRoute::FleetLibraryRevisions => Some(
            get(handler::library::list::history::<D>)
                .post(handler::library::publish_revision::revision::<D>),
        ),
        WorkspaceRoute::FleetLibraryRevision => Some(get(handler::library::read::revision::<D>)),
        WorkspaceRoute::Members => Some(get(tenant::member::in_workspace::<D>)),
    }
}

/// The private-authoring half of [`workspace_handler_for`], before the
/// no-store marking every one of them carries. `None` for a route that is
/// not an authoring route.
fn draft_handler_for<D: Services>(verb: WorkspaceRoute) -> Option<MethodRouter<Arc<D>>> {
    use handler::draft::{message, models, patch, read, stream, write};
    match verb {
        WorkspaceRoute::Models => Some(get(models::list::<D>)),
        WorkspaceRoute::Drafts => Some(get(read::list::<D>).post(write::create::<D>)),
        WorkspaceRoute::Draft => Some(
            get(read::one::<D>)
                .patch(patch::patch::<D>)
                .delete(write::delete::<D>),
        ),
        WorkspaceRoute::DraftMessages => Some(get(message::list::<D>).post(message::send::<D>)),
        WorkspaceRoute::DraftStream => Some(get(stream::open::<D>)),
        WorkspaceRoute::DraftCancellations => Some(post(message::cancel::<D>)),
        WorkspaceRoute::FleetLibrary
        | WorkspaceRoute::FleetLibraryOwned
        | WorkspaceRoute::FleetLibraryItem
        | WorkspaceRoute::FleetLibraryRevisions
        | WorkspaceRoute::FleetLibraryRevision
        | WorkspaceRoute::Fleets
        | WorkspaceRoute::Secrets
        | WorkspaceRoute::Secret
        | WorkspaceRoute::Events
        | WorkspaceRoute::EventsStream
        | WorkspaceRoute::Onboarding
        | WorkspaceRoute::Preference
        | WorkspaceRoute::Approvals
        | WorkspaceRoute::Approval
        | WorkspaceRoute::ApprovalResolve
        | WorkspaceRoute::Members => None,
    }
}

/// Selects the handler for a route scoped to one fleet.
pub fn fleet_handler_for<D: Services>(verb: FleetRoute) -> Option<MethodRouter<Arc<D>>> {
    match verb {
        FleetRoute::Detail => Some(
            get(fleet::native::read::one::<D>)
                .patch(fleet::native::patch::change::<D>)
                .delete(fleet::detail::purge::<D>),
        ),
        FleetRoute::AuthoringContext => Some(handler::draft::private(get(
            fleet::native::context::read::<D>,
        ))),
        FleetRoute::Revisions => Some(post(fleet::native::revision::apply::<D>)),
        FleetRoute::Events => Some(get(handler::event::fleet_list::<D>)),
        FleetRoute::Tasks => Some(get(fleet::tasks::list::<D>)),
        FleetRoute::Task => Some(get(fleet::tasks::one::<D>).patch(fleet::tasks::change::<D>)),
        FleetRoute::Event => Some(get(handler::event::detail::<D>)),
        FleetRoute::ToolCall => Some(get(handler::event::tool_call::read::<D>)),
        FleetRoute::Grants => Some(get(handler::grant::list::<D>)),
        FleetRoute::Grant => Some(delete(handler::grant::revoke::<D>)),
        FleetRoute::Memories => Some(get(fleet::memory::list::<D>)),
        FleetRoute::Memory => Some(delete(fleet::memory::forget::<D>)),
        FleetRoute::MemoryAccess => Some(patch(fleet::memory_access::set::<D>)),
        FleetRoute::Messages => {
            Some(get(fleet::message::thread::<D>).post(fleet::message_steer::steer::<D>))
        }
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
        ConnectorRoute::Complete => Some(post(handler::connector::callback::complete::<D>)),
        ConnectorRoute::Events => None,
    }
}
