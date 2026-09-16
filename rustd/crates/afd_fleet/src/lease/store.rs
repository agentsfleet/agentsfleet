//! The lease store: one pool, one entropy source, and the verbs over both.
//!
//! The same shape as [`crate::runner::Runners`], for the same reason. Zig
//! passes a `*pg.Conn` into every `affinity.zig` function because it has no
//! way to own one — the caller acquires, defers the release, and hands the
//! borrow down. Ported literally that becomes a set of free functions taking
//! `&Db`, which reads fine and gives away the property this crate is built on:
//! [`Leases::pool`] is `pub(crate)`, so nothing outside can run a statement
//! that is not in [`crate::sql`], and the side-by-side parity read of that
//! module stays meaningful (Invariant 5).
//!
//! So the pool is OWNED here and the verbs are methods, split one concern per
//! file — [`super::affinity`] is the claim and the fence, and the modules
//! beside it add the gates and the row.

use afd_admission::Admissions;
use afd_crypto::entropy::Entropy;
use afd_db::Db;
use afd_dragonfly::{FleetStreams, OutboundQueue, ReadyCursor, ReadyIndex, ReadyPrefix, Redis};

/// Lease-plane reads and writes, over the api-role pool and the queue.
///
/// Both datastores, because a lease is the one verb that cannot be served from
/// either alone: the claim and the row are Postgres, the readiness index and
/// the event stream are Redis, and the ordering between them is the whole
/// design. Splitting them across two stores would let a caller take a claim
/// without being able to read the event it is claiming FOR.
///
/// Cheap to clone: `Db` is a handle over an `Arc`-backed pool and `Redis` is a
/// cloneable connection manager, so every clone shares one connection set
/// rather than opening a second.
///
/// The entropy source is the third: issuing a lease mints two identifiers, and
/// they are drawn through the workspace's one entropy surface rather than a
/// second call to the operating system.
///
/// The cursor is the fourth, and the reason every clone shares it: the
/// readiness index is partitioned, a poll reads the partition the cursor
/// names, and a cursor per handle would let every handle start at the same
/// partition and leave the rest to luck. One counter per process is what
/// makes a rotation of polls reach every partition.
#[derive(Debug, Clone)]
pub struct Leases {
    database: Db,
    queue: Redis,
    entropy: Entropy,
    cursor: ReadyCursor,
    ready_prefix: ReadyPrefix,
}

impl Leases {
    /// A store reading and writing through `database` and `queue`.
    #[must_use]
    pub fn new(database: Db, queue: Redis, entropy: Entropy) -> Self {
        Self {
            database,
            queue,
            entropy,
            cursor: ReadyCursor::new(),
            ready_prefix: ReadyPrefix::production(),
        }
    }

    /// The same store, polling the readiness index `prefix` names.
    ///
    /// A test seam, and one the suites cannot do without: the empty-poll
    /// property — that a partition holding nothing costs no Postgres round
    /// trip — is unobservable on the index every other writer shares, because
    /// on a lane that has seeded hundreds of fleets no partition is ever
    /// empty. [`ReadyPrefix`] is what gates the minting; this only chooses.
    #[cfg(feature = "test-util")]
    #[must_use]
    pub fn with_ready_prefix(mut self, prefix: ReadyPrefix) -> Self {
        self.ready_prefix = prefix;
        self
    }

    /// The entropy source, for the sibling module that mints a lease's
    /// identifiers.
    ///
    /// `pub(crate)` for the same reason [`Leases::pool`] is.
    pub(crate) const fn entropy(&self) -> &Entropy {
        &self.entropy
    }

    /// The readiness index, bound to this store's connection.
    ///
    /// Built per call rather than held: it is a zero-cost view over the same
    /// handle, and constructing it here keeps [`Leases::queue`] private for the
    /// same reason [`Leases::pool`] is.
    pub(crate) fn ready(&self) -> ReadyIndex {
        ReadyIndex::under(self.queue.clone(), self.ready_prefix.clone())
    }

    /// The partition cursor every poll through this store turns.
    pub(crate) const fn cursor(&self) -> &ReadyCursor {
        &self.cursor
    }

    /// The fleet event streams, bound to this store's connection.
    pub(crate) fn streams(&self) -> FleetStreams {
        FleetStreams::new(self.queue.clone())
    }

    /// The outbound delivery queue, bound to this store's connection.
    ///
    /// Constructed per call for the reason [`Leases::streams`] is: the handle is
    /// a thin wrapper over the shared connection, and keeping `queue` private
    /// means no caller can reach past the verbs this store exposes.
    pub(crate) fn outbound(&self) -> OutboundQueue {
        OutboundQueue::new(self.queue.clone())
    }

    /// The admission ledger, over the same pool, queue and entropy.
    ///
    /// Built per call like the two views above: the ledger is three handles
    /// this store already holds, and a second copy kept beside them would be
    /// a second thing that could disagree about which pool it reads.
    pub(crate) fn admissions(&self) -> Admissions {
        Admissions::new(
            self.database.clone(),
            self.queue.clone(),
            self.entropy.clone(),
        )
    }

    /// The pool this store reads through, for the sibling modules that add
    /// verbs to [`Leases`] in their own files.
    ///
    /// `pub(crate)`, not `pub`: the pool is an implementation detail of this
    /// crate, and handing it out would let a caller run a statement that is not
    /// in [`crate::sql`].
    pub(crate) const fn pool(&self) -> &Db {
        &self.database
    }
}
