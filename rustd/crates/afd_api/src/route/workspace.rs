//! Routes addressed by workspace alone.
//!
//! The fleet-addressed half lives in [`super::fleet`]. What is left here is
//! the workspace's own surface: its secrets, its event tail, its approval
//! inbox, and the preferences a person sets on it.

use afd_auth::Scope;

use super::path::workspace_path;
use super::{Guard, NONE, RouteClass, RouteMeta, Scopes};

const FLEET_READ: &[Scope] = &[Scope::FleetRead];
const FLEET_WRITE: &[Scope] = &[Scope::FleetWrite];
const SECRET_READ: &[Scope] = &[Scope::SecretRead];
const SECRET_WRITE: &[Scope] = &[Scope::SecretWrite];
const APPROVAL_READ: &[Scope] = &[Scope::ApprovalRead];
const APPROVAL_RESOLVE: &[Scope] = &[Scope::ApprovalResolve];
const LIBRARY_WRITE: &[Scope] = &[Scope::LibraryWrite];

/// A route addressed by workspace only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WorkspaceRoute {
    /// The workspace's fleet-library catalogue.
    FleetLibrary,
    /// The workspace's fleets.
    Fleets,
    /// The workspace's secrets.
    Secrets,
    /// One secret, by name.
    Secret,
    /// The workspace's event history.
    Events,
    /// The workspace's live event stream.
    EventsStream,
    /// Onboarding state.
    Onboarding,
    /// The workspace's preferences.
    Preferences,
    /// One preference.
    Preference,
    /// The approval inbox.
    Approvals,
    /// One approval gate.
    Approval,
    /// Deciding an approval gate.
    ApprovalResolve,
}

impl WorkspaceRoute {
    /// Every workspace route.
    pub const ALL: &'static [Self] = &[
        Self::FleetLibrary,
        Self::Fleets,
        Self::Secrets,
        Self::Secret,
        Self::Events,
        Self::EventsStream,
        Self::Onboarding,
        Self::Preferences,
        Self::Preference,
        Self::Approvals,
        Self::Approval,
        Self::ApprovalResolve,
    ];

    /// Viewing the inbox and deciding a gate are separate capabilities on
    /// purpose: reading what is waiting is not the authority to resolve it.
    ///
    /// Onboarding and preferences carry none — their object is the caller's
    /// own workspace state, and ownership is the check that matters.
    #[must_use]
    pub const fn meta(self) -> RouteMeta {
        let api = RouteClass::Api;
        let (class, template, scopes) = match self {
            Self::FleetLibrary => (
                api,
                workspace_path!("/fleet-libraries"),
                Scopes::rw(FLEET_READ, LIBRARY_WRITE),
            ),
            Self::Fleets => (
                api,
                workspace_path!("/fleets"),
                Scopes::rw(FLEET_READ, FLEET_WRITE),
            ),
            Self::Secrets => (
                api,
                workspace_path!("/secrets"),
                Scopes::rw(SECRET_READ, SECRET_WRITE),
            ),
            Self::Secret => (
                api,
                workspace_path!("/secrets/{name}"),
                Scopes::Always(SECRET_WRITE),
            ),
            Self::Events => (api, workspace_path!("/events"), Scopes::Always(FLEET_READ)),
            Self::EventsStream => (
                RouteClass::Stream,
                workspace_path!("/events/stream"),
                Scopes::Always(FLEET_READ),
            ),
            Self::Onboarding => (api, workspace_path!("/onboarding"), Scopes::Always(NONE)),
            Self::Preferences => (api, workspace_path!("/preferences"), Scopes::Always(NONE)),
            Self::Preference => (
                api,
                workspace_path!("/preferences/{pref_key}"),
                Scopes::Always(NONE),
            ),
            Self::Approvals => (
                api,
                workspace_path!("/approvals"),
                Scopes::Always(APPROVAL_READ),
            ),
            Self::Approval => (
                api,
                workspace_path!("/approvals/{gate_id}"),
                Scopes::Always(APPROVAL_READ),
            ),
            // Two segments, where the Zig daemon spelled one
            // (`{gate_id}:approve`). A router binds one parameter per segment,
            // so the colon form could not be told apart from the detail read
            // above — and the two carry different capabilities, which a single
            // mounted path could not express. The decision moved into its own
            // segment rather than the scope model giving way.
            Self::ApprovalResolve => (
                api,
                workspace_path!("/approvals/{gate_id}/{decision}"),
                Scopes::Always(APPROVAL_RESOLVE),
            ),
        };
        RouteMeta::new(Guard::Bearer, class, template, scopes)
    }
}
