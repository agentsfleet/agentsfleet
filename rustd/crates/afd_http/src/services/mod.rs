//! What an HTTP HANDLER reaches for, as distinct from what `/readyz` consults.
//!
//! Two traits over one state value, split where the seam actually is.
//! [`crate::router::Dependencies`] answers "can this instance take traffic",
//! which is a question about connections; this one answers "what does this verb
//! act through", which is a question about services. A probe that grew a
//! `runners()` method would be asking a readiness check to know about the
//! runner plane.
//!
//! One file per plane, because the seams are one per plane too: the runner
//! plane's verbs in [`leasing`], the device-flow login in [`device_flow`], the
//! tenant's own credentials and ownership in [`tenant`], the tenant's money in
//! [`billing`], and a workspace's fleets in [`fleets`]. Each trait is
//! re-exported here, so a handler still names `crate::services::TenantKeys` and
//! never a file.
//!
//! # Why the state is a trait and not a struct
//!
//! The authenticator's concrete type carries three parameters — a directory, a
//! capability source, and a token verifier — and every one of them is chosen by
//! the binary, not by this crate. A concrete state struct would put all three
//! on `build`, on every handler signature, and on every test fixture. One
//! associated type collapses them, and the request path still costs no virtual
//! call because the trait is taken as a generic parameter (`M-DI-HIERARCHY`).
//!
//! # Why the clock is here
//!
//! `afd_core::clock` asks callers to take an instant as a PARAMETER wherever
//! the decision can be handed one, and reserves injection for a long-lived
//! owner that reads repeatedly. The router is that owner: it lives for the
//! process, and every verb under it needs the instant its writes are stamped
//! with. Reading the wall clock inside each handler instead would put a
//! non-deterministic call in the one place a test most needs to pin.

mod approval;
mod billing;
mod catalogue;
mod connector;
mod device_flow;
mod event;
mod fleets;
mod grant;
mod ingress;
mod leasing;
mod memory;
mod model_entry;
mod preference;
mod provider;
mod schedule;
mod signup;
mod tenant;
mod vault;

pub use self::approval::WorkspaceApprovals;
pub use self::billing::TenantBilling;
pub use self::catalogue::ModelCatalogue;
pub use self::connector::WorkspaceConnectors;
pub use self::device_flow::DeviceFlow;
pub use self::event::{FleetSteering, WorkspaceEvents};
pub use self::fleets::WorkspaceFleets;
pub use self::grant::FleetGrants;
pub use self::ingress::{APPROVAL_IDENTITY, WebhookIngress};
pub use self::leasing::Leasing;
pub use self::memory::FleetMemories;
pub use self::model_entry::TenantModelEntries;
pub use self::preference::WorkspacePreferences;
pub use self::provider::TenantProviders;
pub use self::schedule::{FleetSchedules, SchedulePlane};
pub use self::signup::{
    Bootstrapped, IdentityWebhookSecret, NewAccount, Signups, personal_tenant_name,
};
pub use self::tenant::{TenantKeys, TenantWorkspaces, TerminalCredentials, WorkspaceOwnership};
pub use self::vault::WorkspaceSecrets;

mod contract;
mod tenant_surface;

pub use self::contract::Services;
pub use self::tenant_surface::TenantSurface;
