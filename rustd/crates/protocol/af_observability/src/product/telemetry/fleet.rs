//! The two server steps of the first-message-to-fleet funnel
//! (`docs/architecture/product_analytics.md` §"Daemon events").
//!
//! # No field can hold what a person wrote
//!
//! Every identifier is a [`Uuid7`] and every word is a closed enum or a
//! `&'static str` the reporting crate spells as a constant. A message, a
//! fleet's name, a secret's name, a repository or a URL is runtime text, and no
//! field here has a type that takes runtime text — so a widened call site fails
//! to compile rather than leaking.
//!
//! # A step is reported once, after its commit
//!
//! The reporter decides that, not this type: a create reports only after the
//! transaction that wrote the fleet commits, and never for a replay.

use af_core::id::Uuid7;

/// Where a fleet's first message came from, as its creator's credential says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FleetOrigin {
    /// A terminal: a command-line credential or an API key.
    Cli,
    /// A browser session.
    Dashboard,
}

impl FleetOrigin {
    /// The word the funnel splits on.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cli => "cli",
            Self::Dashboard => "dashboard",
        }
    }
}

/// What happened to a fleet's grant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessChange {
    /// The fleet asked for it, and a card was raised.
    Requested,
    /// A person approved it.
    Approved,
    /// A person denied it.
    Denied,
    /// A person took an approved grant back.
    Revoked,
}

impl AccessChange {
    /// The word the funnel splits on.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Requested => "requested",
            Self::Approved => "approved",
            Self::Denied => "denied",
            Self::Revoked => "revoked",
        }
    }
}

/// Which funnel step happened, with the opaque identifiers it carries.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum FleetStep {
    /// A first message created the fleet.
    Created {
        /// The fleet the create wrote.
        fleet_id: Uuid7,
        /// Where the message came from.
        origin: FleetOrigin,
    },
    /// A grant was requested, approved, denied or revoked.
    AccessChanged {
        /// The fleet the grant is for.
        fleet_id: Uuid7,
        /// `secret`, `integration` or `host`, as `connector.integration_grants.grant_type`
        /// spells it.
        kind: &'static str,
        /// What happened to it.
        change: AccessChange,
    },
}

impl FleetStep {
    /// The event name `PostHog` receives, byte-stable for the funnel that
    /// matches on it.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match *self {
            Self::Created { .. } => "fleet_created",
            Self::AccessChanged { .. } => "fleet_access_changed",
        }
    }
}
