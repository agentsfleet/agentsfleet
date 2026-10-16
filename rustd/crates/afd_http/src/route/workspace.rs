//! Routes addressed by a workspace alone.
//!
//! The fleet-addressed half lives in [`super::fleet`]. What is left here is
//! the workspace's own surface: its secrets, its event tail, its approval
//! inbox, and the preferences a person sets on it.

use afd_auth::Scope;

use super::path::workspace_path;
use super::{Guard, NONE, RouteClass, RouteMeta, Scopes, Verb};

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
    /// Supported model choices from the workspace's billing owner.
    Models,
    /// The authenticated creator's private draft collection.
    Drafts,
    /// One creator-private draft and current candidate.
    Draft,
    /// Private user requests and assistant answers.
    DraftMessages,
    /// Monotone private authoring Stop watermark.
    DraftCancellations,
    /// Creator-private, currently authorized setup progress.
    DraftStream,
    /// The workspace's fleet-library catalogue.
    FleetLibrary,
    /// Owned reusable entries, excluding the platform catalogue.
    FleetLibraryOwned,
    /// One reusable native entry.
    FleetLibraryItem,
    /// Immutable revisions of an entry.
    FleetLibraryRevisions,
    /// One pinned immutable revision.
    FleetLibraryRevision,
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
    /// One preference.
    Preference,
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
        Self::Drafts,
        Self::Draft,
        Self::DraftMessages,
        Self::DraftCancellations,
        Self::DraftStream,
        Self::FleetLibrary,
        Self::FleetLibraryOwned,
        Self::FleetLibraryItem,
        Self::FleetLibraryRevisions,
        Self::FleetLibraryRevision,
        Self::Fleets,
        Self::Secrets,
        Self::Secret,
        Self::Events,
        Self::EventsStream,
        Self::Onboarding,
        Self::Preference,
        Self::Approvals,
        Self::Approval,
        Self::ApprovalResolve,
        Self::Members,
    ];

    /// The verbs this route identity serves.
    ///
    /// A secret is replaced whole rather than patched — a field-wise edit on a
    /// credential would let a caller move the projection out from under the
    /// ciphertext beside it — so `Secret` carries `PUT` and no `PATCH`. A
    /// preference is the same shape for the same reason: one document, written
    /// whole.
    #[must_use]
    pub const fn verbs(self) -> &'static [Verb] {
        match self {
            Self::Models
            | Self::Events
            | Self::EventsStream
            | Self::DraftStream
            | Self::Onboarding
            | Self::Approvals
            | Self::Approval
            | Self::FleetLibraryOwned
            | Self::FleetLibraryRevision
            | Self::Members => &[Verb::Get],
            Self::Preference => &[Verb::Put],
            Self::ApprovalResolve | Self::DraftCancellations => &[Verb::Post],
            Self::Drafts | Self::DraftMessages => &[Verb::Get, Verb::Post],
            Self::Draft => &[Verb::Get, Verb::Patch, Verb::Delete],
            Self::FleetLibrary | Self::FleetLibraryRevisions | Self::Fleets | Self::Secrets => {
                &[Verb::Get, Verb::Post]
            }
            Self::Secret => &[Verb::Put, Verb::Delete],
            Self::FleetLibraryItem => &[Verb::Get, Verb::Delete],
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
            Self::DraftStream | Self::EventsStream => RouteClass::Stream,
            Self::Models
            | Self::Drafts
            | Self::Draft
            | Self::DraftMessages
            | Self::DraftCancellations
            | Self::FleetLibrary
            | Self::FleetLibraryOwned
            | Self::FleetLibraryItem
            | Self::FleetLibraryRevisions
            | Self::FleetLibraryRevision
            | Self::Fleets
            | Self::Secrets
            | Self::Secret
            | Self::Events
            | Self::Onboarding
            | Self::Preference
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
            Self::Drafts => workspace_path!("/drafts"),
            Self::Draft => workspace_path!("/drafts/{draft_id}"),
            Self::DraftMessages => workspace_path!("/drafts/{draft_id}/messages"),
            Self::DraftCancellations => workspace_path!("/drafts/{draft_id}/cancellations"),
            Self::DraftStream => workspace_path!("/drafts/{draft_id}/events/stream"),
            Self::FleetLibrary => workspace_path!("/fleet-libraries"),
            Self::FleetLibraryOwned => workspace_path!("/fleet-libraries/owned"),
            Self::FleetLibraryItem => workspace_path!("/fleet-libraries/{library_id}"),
            Self::FleetLibraryRevisions => {
                workspace_path!("/fleet-libraries/{library_id}/revisions")
            }
            Self::FleetLibraryRevision => {
                workspace_path!("/fleet-libraries/{library_id}/revisions/{revision_id}")
            }
            Self::Fleets => workspace_path!("/fleets"),
            Self::Secrets => workspace_path!("/secrets"),
            Self::Secret => workspace_path!("/secrets/{name}"),
            Self::Events => workspace_path!("/events"),
            Self::EventsStream => workspace_path!("/events/stream"),
            Self::Onboarding => workspace_path!("/onboarding"),
            Self::Preference => workspace_path!("/preferences/{pref_key}"),
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
    ///
    /// Onboarding and preferences carry none — their object is the caller's
    /// own workspace state, and ownership is the check that matters.
    const fn scopes(self) -> Scopes {
        match self {
            Self::Models
            | Self::FleetLibraryOwned
            | Self::FleetLibraryRevision
            | Self::Events
            | Self::EventsStream
            | Self::Members => Scopes::Always(FLEET_READ),
            Self::Drafts
            | Self::Draft
            | Self::DraftMessages
            | Self::DraftCancellations
            | Self::DraftStream => Scopes::Always(FLEET_WRITE),
            Self::FleetLibrary | Self::FleetLibraryItem | Self::FleetLibraryRevisions => {
                Scopes::rw(FLEET_READ, LIBRARY_WRITE)
            }
            Self::Fleets => Scopes::rw(FLEET_READ, FLEET_WRITE),
            Self::Secrets => Scopes::rw(SECRET_READ, SECRET_WRITE),
            Self::Secret => Scopes::Always(SECRET_WRITE),
            Self::Onboarding | Self::Preference => Scopes::Always(NONE),
            Self::Approvals | Self::Approval => Scopes::Always(APPROVAL_READ),
            Self::ApprovalResolve => Scopes::Always(APPROVAL_RESOLVE),
        }
    }
}
