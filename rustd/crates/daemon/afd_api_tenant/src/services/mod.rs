//! What a tenant handler acts through: [`TenantPlane`], and the seams only
//! this plane reaches.
//!
//! The seams another plane also reaches live in [`afd_http::services`]; the
//! private import below lets a handler name them beside these as
//! `crate::services::…`.

mod billing;
mod catalogue;
mod device_flow;
mod grant;
mod mail;
mod memory;
mod model_entry;
mod plane;
mod preference;
mod provider;
mod team;
mod tenant;
mod vault;

pub use self::billing::TenantBilling;
pub use self::catalogue::ModelCatalogue;
pub use self::device_flow::DeviceFlow;
pub use self::grant::FleetGrants;
pub use self::mail::InviteMail;
pub use self::memory::FleetMemories;
pub use self::model_entry::TenantModelEntries;
pub use self::plane::TenantPlane;
pub use self::preference::WorkspacePreferences;
pub use self::provider::TenantProviders;
pub use self::team::TenantTeam;
pub use self::tenant::{TenantKeys, TenantWorkspaces, TerminalCredentials};
pub use self::vault::WorkspaceSecrets;

pub(crate) use afd_http::services::{
    APPROVAL_IDENTITY, FleetSchedules, FleetSteering, WebhookIngress, WorkspaceApprovals,
    WorkspaceConnectors, WorkspaceFleets, WorkspaceOwnership, WorkspaceTurns,
};
