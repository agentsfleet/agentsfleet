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

use afd_crypto::entropy::Entropy;
use afd_db::Db;
use afd_redis::{FleetStreams, ReadyIndex, Redis};

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
#[derive(Debug, Clone)]
pub struct Leases {
    database: Db,
    queue: Redis,
    entropy: Entropy,
}

impl Leases {
    /// A store reading and writing through `database` and `queue`.
    #[must_use]
    pub const fn new(database: Db, queue: Redis, entropy: Entropy) -> Self {
        Self {
            database,
            queue,
            entropy,
        }
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
        ReadyIndex::new(self.queue.clone())
    }

    /// The fleet event streams, bound to this store's connection.
    pub(crate) fn streams(&self) -> FleetStreams {
        FleetStreams::new(self.queue.clone())
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
