//! The lease store: one pool, one entropy source, and the verbs over both.
//!
//! The same shape as [`afd_runner::Runners`], for the same reason. A set of
//! free functions taking `&Db` would read fine and give away the property this
//! crate is built on: `Leases::pool` is `pub(crate)`, so nothing outside can
//! run a statement that is not in [`crate::lease::sql`], and that module stays
//! the one place a reviewer reads every statement the plane runs.
//!
//! So the pool is OWNED here and the verbs are methods, split one concern per
//! file — [`super::affinity`] is the claim and the fence, and the modules
//! beside it add the gates and the row.

use afd_crypto::entropy::Entropy;
use afd_db::Db;
use afd_dragonfly::{Doorbell, Dragonfly, FleetStreams, OutboundQueue};

/// Queue selection and lease writes use Postgres. Dragonfly carries the live
/// tail, outbound delivery and disposable recent-work notifications.
#[derive(Debug, Clone)]
pub struct Leases {
    database: Db,
    queue: Dragonfly,
    entropy: Entropy,
    doorbell: Doorbell,
    ticket: Option<afd_runner::capacity::choice::Ticket>,
}

impl Leases {
    /// A store reading and writing through `database` and `queue`.
    #[must_use]
    pub fn new(database: Db, queue: Dragonfly, entropy: Entropy) -> Self {
        Self {
            database,
            queue,
            entropy,
            doorbell: Doorbell::new(),
            ticket: None,
        }
    }

    pub(super) fn with_ticket(
        mut self,
        ticket: Option<afd_runner::capacity::choice::Ticket>,
    ) -> Self {
        self.ticket = ticket;
        self
    }

    pub(super) const fn ticket(&self) -> Option<afd_runner::capacity::choice::Ticket> {
        self.ticket
    }

    /// The same store, taking recent fleet hints from `doorbell`.
    ///
    /// Without one nothing rings, and polls find fresh work through the
    /// Postgres probe alone. The daemon hands in the doorbell its hub listens for.
    #[must_use]
    pub fn with_doorbell(mut self, doorbell: Doorbell) -> Self {
        self.doorbell = doorbell;
        self
    }

    /// The entropy source, for the sibling module that mints a lease's
    /// identifiers.
    ///
    /// `pub(crate)` for the same reason [`Leases::pool`] is.
    pub(crate) const fn entropy(&self) -> &Entropy {
        &self.entropy
    }

    /// The doorbell every poll through this store takes rung partitions from.
    pub(crate) const fn doorbell(&self) -> &Doorbell {
        &self.doorbell
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
