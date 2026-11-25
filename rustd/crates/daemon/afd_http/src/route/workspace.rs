//! Routes addressed by a workspace alone.
//!
//! The fleet-addressed half lives in [`super::fleet`]. What is left here is
//! the workspace's own surface: its fleets, its model choices, its secrets, its
//! turn tail, its approval inbox, and its onboarding checklist.

use afd_auth::Scope;

use super::path::workspace_path;
use super::{Guard, NONE, RouteClass, RouteMeta, Scopes, Verb};

const FLEET_READ: &[Scope] = &[Scope::FleetRead];
const FLEET_WRITE: &[Scope] = &[Scope::FleetWrite];
const SECRET_READ: &[Scope] = &[Scope::SecretRead];
const SECRET_WRITE: &[Scope] = &[Scope::SecretWrite];
const APPROVAL_READ: &[Scope] = &[Scope::ApprovalRead];
const APPROVAL_RESOLVE: &[Scope] = &[Scope::ApprovalResolve];

/// A route addressed by workspace only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WorkspaceRoute {
    /// Supported model choices from the workspace's billing owner.
    Models,
    /// The workspace's fleets: list them, or start one with its first message.
    Fleets,
    /// The workspace's secrets.
    Secrets,
    /// One secret, by name.
    Secret,
    /// The workspace's turn history.
    Turns,
    /// The workspace's live turn stream.
    TurnsStream,
    /// Onboarding state.
    Onboarding,
    /// The approval inbox.
    Approvals,
    /// One approval gate.
    Approval,
    /// Deciding an approval gate.
    ApprovalResolve,
    /// The members of the workspace's account, by name.
    Members,
}

impl WorkspaceRoute {
    /// Every workspace route.
    pub const ALL: &'static [Self] = &[
        Self::Models,
        Self::Fleets,
        Self::Secrets,
        Self::Secret,
        Self::Turns,
        Self::TurnsStream,
        Self::Onboarding,
        Self::Approvals,
        Self::Approval,
        Self::ApprovalResolve,
        Self::Members,
    ];

    /// The verbs this route identity serves.
    ///
    /// A secret is replaced whole rather than patched — a field-wise edit on a
    /// credential would let a caller move the projection out from under the
    /// ciphertext beside it — so `Secret` carries `PUT` and no `PATCH`.
    #[must_use]
    pub const fn verbs(self) -> &'static [Verb] {
        match self {
            Self::Models
            | Self::Turns
            | Self::TurnsStream
            | Self::Onboarding
            | Self::Approvals
            | Self::Approval
            | Self::Members => &[Verb::Get],
            Self::ApprovalResolve => &[Verb::Post],
            Self::Fleets | Self::Secrets => &[Verb::Get, Verb::Post],
            Self::Secret => &[Verb::Put, Verb::Delete],
        }
    }

    /// The route's guard, class, template and scope rungs, as one table row.
    #[must_use]
    pub const fn meta(self) -> RouteMeta {
        RouteMeta::new(Guard::Bearer, self.class(), self.template(), self.scopes())
    }

    /// The two live tails stream; everything else answers once. Exhaustive
    /// rather than a wildcard, so a new route has to say which it is.
    const fn class(self) -> RouteClass {
        match self {
            Self::TurnsStream => RouteClass::Stream,
            Self::Models
            | Self::Fleets
            | Self::Secrets
            | Self::Secret
            | Self::Turns
            | Self::Onboarding
            | Self::Approvals
            | Self::Approval
            | Self::ApprovalResolve
            | Self::Members => RouteClass::Api,
        }
    }

    /// Where the route is mounted, under the workspace.
    const fn template(self) -> &'static str {
        match self {
            Self::Models => workspace_path!("/models"),
            Self::Fleets => workspace_path!("/fleets"),
            Self::Secrets => workspace_path!("/secrets"),
            Self::Secret => workspace_path!("/secrets/{name}"),
            Self::Turns => workspace_path!("/turns"),
            Self::TurnsStream => workspace_path!("/turns/stream"),
            Self::Onboarding => workspace_path!("/onboarding"),
            Self::Approvals => workspace_path!("/approvals"),
            Self::Approval => workspace_path!("/approvals/{gate_id}"),
            // Two segments rather than one (`{gate_id}:approve`). A router
            // binds one parameter per segment, so the colon form could not be
            // told apart from the detail read above — and the two carry
            // different capabilities, which a single mounted path could not
            // express. The decision takes its own segment rather than the scope
            // model giving way.
            Self::ApprovalResolve => workspace_path!("/approvals/{gate_id}/{decision}"),
            Self::Members => workspace_path!("/members"),
        }
    }

    /// Viewing the inbox and deciding a gate are separate capabilities on
    /// purpose: reading what is waiting is not the authority to resolve it.
    /// Approving or denying also needs a signed-in person, which the handler
    /// asks of the decision the path names.
    ///
    /// Onboarding carries none — its object is the caller's own workspace
    /// state, and ownership is the check that matters.
    const fn scopes(self) -> Scopes {
        match self {
            Self::Models | Self::Turns | Self::TurnsStream | Self::Members => {
                Scopes::Always(FLEET_READ)
            }
            Self::Fleets => Scopes::rw(FLEET_READ, FLEET_WRITE),
            Self::Secrets => Scopes::rw(SECRET_READ, SECRET_WRITE),
            Self::Secret => Scopes::Always(SECRET_WRITE),
            Self::Onboarding => Scopes::Always(NONE),
            Self::Approvals | Self::Approval => Scopes::Always(APPROVAL_READ),
            Self::ApprovalResolve => Scopes::Always(APPROVAL_RESOLVE),
        }
    }
}
