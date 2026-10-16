//! The five server steps of the private-chat-to-fleet funnel
//! (`docs/architecture/product_analytics.md` §"Private chat to fleet funnel").
//!
//! # No field can hold what a person wrote
//!
//! Every identifier is a [`Uuid7`] and every word is a `&'static str` the
//! reporting crate spells as a constant. A message, a Definition, a fleet name,
//! a URL or a secret is runtime text, and no field here has a type that takes
//! runtime text — so a widened call site fails to compile rather than leaking.
//!
//! # A step is reported once, after its commit
//!
//! The reporter decides that, not this type: `afd_authoring` reports a step
//! only after the transaction that made it true commits, and never for a replay
//! of an operation that already committed.

use afd_core::id::Uuid7;

/// Which funnel step happened, with the opaque identifiers that step carries.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum AuthoringStep {
    /// A new private message was durably accepted.
    MessageAccepted {
        /// The private draft it was sent to.
        draft_id: Uuid7,
    },
    /// An accepted message reached its durable terminal state.
    MessageFinished {
        /// The private draft it belongs to.
        draft_id: Uuid7,
        /// `completed`, `cancelled`, `failed` or `timed_out`.
        state: &'static str,
    },
    /// Setup activated the fleet.
    SetupFinished {
        /// The fleet setup activated.
        fleet_id: Uuid7,
    },
    /// A Kill cancelled setup before it activated the fleet.
    SetupCancelled {
        /// The fleet whose setup was cancelled.
        fleet_id: Uuid7,
    },
    /// A new Library revision was committed.
    LibraryPublished {
        /// The Library entry.
        library_id: Uuid7,
        /// The revision this publication committed.
        revision_id: Uuid7,
        /// `entry` for a first publication, `revision` for a later one.
        kind: &'static str,
    },
}

impl AuthoringStep {
    /// The event name `PostHog` receives, byte-stable for the funnel that
    /// matches on it.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match *self {
            Self::MessageAccepted { .. } => "draft_message_accepted",
            Self::MessageFinished { .. } => "draft_message_finished",
            Self::SetupFinished { .. } => "fleet_setup_finished",
            Self::SetupCancelled { .. } => "fleet_setup_cancelled",
            Self::LibraryPublished { .. } => "fleet_library_published",
        }
    }
}
