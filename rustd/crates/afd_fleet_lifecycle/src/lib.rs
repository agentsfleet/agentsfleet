//! A workspace's fleets as one shared store: list, live set, counters, purge.
//!
//! # Why this is its own crate
//!
//! It is the lifecycle AROUND a fleet, where `afd_fleet` is the runner plane
//! that acts on one — leases, gates, budgets, memory. The two never call each
//! other; they meet at `fleet.fleets`, one writing the row and the other reading
//! it, which is a table and not a dependency.
//!
//! Setup is not here. Every fleet is written by the native setup authority,
//! `afd_authoring::Creations` — a person's reviewed Save, an explicit Apply,
//! and the daemon's own system setup for a channel's resident — so one parser
//! and one storage shape decide what a fleet may do.
//!
//! # What it does not contain
//!
//! No routing, no HTTP, no extractor. Whether a caller MAY act on a workspace is
//! decided at the edge, in `afd_api`, by a layer mounted from the route's own
//! template. This crate answers what a verb DOES once that decision is made —
//! and re-answers the narrower question of whether the FLEET is in that
//! workspace, in SQL, because that one it can enforce rather than trust.
#![cfg_attr(docsrs, feature(doc_auto_cfg))]
#![deny(unused_crate_dependencies)]
#![deny(clippy::self_named_module_files)]
// Named for the lint's benefit: both are the integration lane's, and
// `unused_crate_dependencies` counts a dev-dependency against the LIB target
// unless the crate root says it knows about them.
#[cfg(test)]
use {afd_crypto as _, tokio as _};

pub mod error;

mod live_set;
mod purge;
mod read;
mod sql;

use afd_db::Db;
use afd_dragonfly::{Dragonfly, FleetStreams, ReadyIndex};

/// The purge's statements, for the lane that runs them against a live role.
///
/// Gated on `test-util` because nothing in production reaches SQL text through
/// this crate's surface. The lane needs them by name rather than by copy: a
/// test that retypes a statement proves the copy, and the pair drifts the
/// moment one side is edited. Bindings rather than `pub use`, because the
/// statements stay `pub(crate)`.
#[cfg(feature = "test-util")]
pub mod purge_statements {
    /// See `crate::sql::LOCK_FLEET`.
    pub const LOCK_FLEET: &str = crate::sql::LOCK_FLEET;
    /// See `crate::sql::DELETE_FLEET_IN_STATUS`.
    pub const DELETE_FLEET_IN_STATUS: &str = crate::sql::DELETE_FLEET_IN_STATUS;
}

pub use self::error::{Error, Result};
pub use self::read::{After, FleetPage, FleetRow, Triggers};

/// The workspace's fleets, as one store.
///
/// Holds Postgres and two views of Dragonfly, because a purge removes the row
/// and then forgets the stream and the readiness mark that hang off it.
#[derive(Debug, Clone)]
pub struct Fleets {
    database: Db,
    streams: FleetStreams,
    ready: ReadyIndex,
    /// One enumeration per workspace per tick, shared by every viewer of it.
    /// See [`live_set`] for why this read is cached where the others are not.
    live_sets: live_set::LiveSets,
}

impl Fleets {
    /// Binds the store to already-connected handles.
    ///
    /// Takes the Dragonfly CONNECTION and builds both views over it, rather
    /// than taking the views: which of them this crate needs is its own
    /// business, and a composition root assembling them would have to be
    /// edited every time that answer changed.
    #[must_use]
    pub fn new(database: Db, queue: Dragonfly) -> Self {
        Self {
            database,
            streams: FleetStreams::new(queue.clone()),
            ready: ReadyIndex::new(queue),
            live_sets: live_set::live_sets(),
        }
    }
}

/// Where a fleet stands in its life.
///
/// A closed enum rather than a string: the statements here bind these as
/// parameters, so a typo is a compile error rather than a predicate that
/// silently matches no row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FleetStatus {
    /// The row exists; its setup checks or its stream may not yet.
    ///
    /// The state every fleet is born in. Setup activation flips it to
    /// [`FleetStatus::Active`] only after a fresh locked recheck.
    Installing,
    /// Leasable. The only status the runner's candidate query admits.
    Active,
    /// Held by the platform's anomaly gate. Never reachable through the API.
    Paused,
    /// Stopped by an operator, and resumable.
    Stopped,
    /// Terminal. A killed fleet answers 404 to every further edit, and is the
    /// only status a purge will delete from.
    Killed,
}

impl FleetStatus {
    /// Every status, in lifecycle order.
    ///
    /// What [`FleetStatus::parse`] searches and what the census label set is
    /// held equal to; a member added here without its label fails that test.
    pub const ALL: [Self; 5] = [
        Self::Installing,
        Self::Active,
        Self::Paused,
        Self::Stopped,
        Self::Killed,
    ];

    /// The stored spelling — the bytes in the column, and on the wire.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Installing => "installing",
            Self::Active => "active",
            Self::Paused => "paused",
            Self::Stopped => "stopped",
            Self::Killed => "killed",
        }
    }

    /// Whether this fleet will take new work.
    ///
    /// `active` and nothing else, exactly as `isRunnable`. The distinction is
    /// load-bearing on the steer: a message accepted for a stopped fleet is a
    /// 202 whose run never happens, so the surface refuses loudly instead —
    /// see the ingress refusal in `handler::fleet::message_steer`.
    #[must_use]
    pub const fn is_runnable(self) -> bool {
        matches!(self, Self::Active)
    }

    /// The status a stored spelling names, if this daemon knows it.
    ///
    /// `None` rather than a default, because a row holding a status this build
    /// does not know is a fact worth refusing over: defaulting it would let a
    /// newer daemon's state read as `installing` here and be flipped out of it.
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|status| status.as_str() == raw)
    }
}

#[cfg(test)]
mod tests {
    use afd_observability::metrics::label::fleet::FleetStatusLabel;

    use super::FleetStatus;

    #[test]
    fn every_status_round_trips_through_its_stored_spelling() {
        for status in FleetStatus::ALL {
            assert_eq!(FleetStatus::parse(status.as_str()), Some(status));
        }
    }

    /// The census label set and this enum agree member for member.
    ///
    /// They cannot share a type — this crate depends on the observability
    /// crate, not the reverse — so the agreement is asserted. Compared as
    /// ORDERED spellings: a status added to one side, renamed on one side, or
    /// reordered on one side all fail here, and each is a census row about to
    /// be wrong.
    #[test]
    fn the_census_label_set_mirrors_the_lifecycle() {
        let lifecycle: Vec<&str> = FleetStatus::ALL
            .iter()
            .map(|status| status.as_str())
            .collect();
        let census: Vec<&str> = FleetStatusLabel::ALL
            .iter()
            .map(|status| status.as_str())
            .collect();
        assert_eq!(
            lifecycle, census,
            "a fleet status the census cannot spell is a fleet the dashboard cannot count"
        );
    }

    #[test]
    fn an_unknown_spelling_is_refused_rather_than_defaulted() {
        // A newer daemon's status must not read as `installing` here — this
        // build would then flip a row out of a state it does not understand.
        assert_eq!(FleetStatus::parse("draining"), None);
        assert_eq!(FleetStatus::parse(""), None);
        assert_eq!(FleetStatus::parse("ACTIVE"), None);
    }
}
