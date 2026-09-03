//! A workspace's fleets: install, read, edit, purge.
//!
//! # Why this is its own crate
//!
//! It is the lifecycle AROUND a fleet, where `afd_fleet` is the runner plane
//! that acts on one — leases, gates, budgets, memory. The two never call each
//! other; they meet at `core.fleets`, one writing the row and the other reading
//! it, which is a table and not a dependency.
//!
//! Folding this in would have gone one of two wrong ways. `afd_fleet` is
//! already 25,500 lines — three and a half times its nearest sibling, and the
//! exact condition that forced `afd_tenant` out of it, where an edit to a
//! 400-line module rebuilt everything. `afd_tenant` is the other candidate and
//! is the wrong shape for a different reason: it would acquire a YAML parser
//! and a Redis stream client that its api-key, credential and login modules
//! never call, and every edit to those would then rebuild both.
//!
//! What is here has exactly two edges out that its neighbours lack —
//! [`afd_fleet_runtime`] for the authored documents, and `afd_redis`'s stream
//! client for the install guarantee — and both are load-bearing here and used
//! nowhere else in the tenant-facing surface.
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
// Named for the lint's benefit: both are the integration lane's, and
// `unused_crate_dependencies` counts a dev-dependency against the LIB target
// unless the crate root says it knows about them.
#[cfg(test)]
use {redis as _, tokio as _};

pub mod error;

mod edit;
mod install;
mod live_set;
mod purge;
mod read;
mod sql;

use afd_crypto::entropy::Entropy;
use afd_db::Db;
use afd_redis::{FleetStreams, ReadyIndex, Redis};
use afd_vault::Directory;

pub use self::edit::{ConfigSource, Patch, Patched, Requested};
pub use self::error::{Error, Result};
pub use self::install::{Install, Installed, LibrarySource};
pub use self::read::{After, FleetDetail, FleetPage, FleetRow, Triggers};

/// The workspace's fleets, as one store.
///
/// Holds Postgres, two views of Redis, and an entropy source, because an
/// install needs all of them in ONE operation: the row is written, the stream
/// and its consumer group are created, and only then is the fleet real. A
/// caller holding the pieces and sequencing them itself is a caller that can
/// get the order wrong, and wrong here means a fleet whose first event arrives
/// before anything exists to read it.
#[derive(Debug, Clone)]
pub struct Fleets {
    database: Db,
    streams: FleetStreams,
    ready: ReadyIndex,
    entropy: Entropy,
    /// One enumeration per workspace per tick, shared by every viewer of it.
    /// See [`live_set`] for why this read is cached where the others are not.
    live_sets: live_set::LiveSets,
    /// The workspace's stored credential NAMES, for the install's pre-flight.
    ///
    /// Built here from the pool this store already holds rather than taken as a
    /// fourth argument: which reads this crate needs is its own business, and a
    /// composition root assembling the pair would be edited every time that
    /// answer changed — the same argument [`Self::new`] makes about the two
    /// Redis views.
    secrets: Directory,
}

impl Fleets {
    /// Binds the store to already-connected handles.
    ///
    /// Takes the Redis CONNECTION and builds both views over it, rather than
    /// taking the views: which of them this crate needs is its own business,
    /// and a composition root assembling the pair would have to be edited every
    /// time that answer changed.
    #[must_use]
    pub fn new(database: Db, queue: Redis, entropy: Entropy) -> Self {
        Self {
            database: database.clone(),
            streams: FleetStreams::new(queue.clone()),
            ready: ReadyIndex::new(queue),
            entropy,
            live_sets: live_set::live_sets(),
            secrets: Directory::new(database),
        }
    }
}

/// Where a fleet stands in its life.
///
/// A closed enum rather than the `[]const u8` the Zig passes around, and the
/// difference shows up twice: the status machine in [`sql::PATCH_FLEET`] binds
/// these as parameters, so a typo is a compile error rather than a predicate
/// that silently matches no row; and [`Requested`] — what an API caller may ASK
/// for — is a separate, smaller type, so `paused` is not a value a request can
/// spell at all. The Zig checks that by hand in `patch_body.validateBody` and
/// would still compile without the check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FleetStatus {
    /// The row exists; its stream may not yet.
    ///
    /// The state a fleet is born in and, in this daemon, one no caller observes
    /// on a successful install: the flip to [`FleetStatus::Active`] happens
    /// inside the install rather than on a worker afterwards. See
    /// [`install`] for why that changed.
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
    /// see the ingress refusal in `handler::fleet::message`.
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
        [
            Self::Installing,
            Self::Active,
            Self::Paused,
            Self::Stopped,
            Self::Killed,
        ]
        .into_iter()
        .find(|status| status.as_str() == raw)
    }
}

#[cfg(test)]
mod tests {
    use super::FleetStatus;

    #[test]
    fn every_status_round_trips_through_its_stored_spelling() {
        for status in [
            FleetStatus::Installing,
            FleetStatus::Active,
            FleetStatus::Paused,
            FleetStatus::Stopped,
            FleetStatus::Killed,
        ] {
            assert_eq!(FleetStatus::parse(status.as_str()), Some(status));
        }
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
