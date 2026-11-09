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

use af_crypto::entropy::Entropy;
use afd_bus::Handle;
use afd_db::Db;
use afd_dragonfly::doorbell::Waited;
use afd_dragonfly::{Doorbell, Dragonfly, OutboundQueue};

/// Queue selection and lease writes use Postgres. Dragonfly carries the live
/// tail, outbound delivery and disposable recent-work notifications.
#[derive(Debug, Clone)]
pub struct Leases {
    database: Db,
    queue: Dragonfly,
    bus: Handle,
    entropy: Entropy,
    doorbell: Doorbell,
    ticket: Option<afd_runner::capacity::choice::Ticket>,
    authority: Option<afd_runner::capacity::authority::Authority>,
}

impl Leases {
    /// A store reading and writing through `database` and `queue`.
    #[must_use]
    pub fn new(database: Db, queue: Dragonfly, bus: Handle, entropy: Entropy) -> Self {
        Self {
            database,
            queue,
            bus,
            entropy,
            doorbell: Doorbell::new(),
            ticket: None,
            authority: None,
        }
    }

    /// Bind streamed lease operations to the credential and owner acquired at opening.
    #[must_use]
    pub fn for_connection(mut self, authority: afd_runner::capacity::authority::Authority) -> Self {
        self.authority = Some(authority);
        self
    }

    /// Lock the runner row through this transaction and count its occupied workers.
    /// Stream grants also recheck the opening credential and connection owner.
    pub(crate) async fn lock_host_capacity(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        runner: &af_core::id::Uuid7,
    ) -> crate::Result<Option<afd_runner::capacity::Capacity>> {
        Ok(match &self.authority {
            Some(authority) => afd_runner::capacity::lock(tx, (runner, authority)).await?,
            None => afd_runner::capacity::lock(tx, runner).await?,
        })
    }

    pub(super) const fn with_ticket(
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

    /// The doorbell every poll through this store takes rung partitions from,
    /// and every held heartbeat waits on.
    #[must_use]
    pub const fn doorbell(&self) -> &Doorbell {
        &self.doorbell
    }

    /// Holds a runner's heartbeat until fresh work is admitted anywhere this
    /// process hears of it, or `deadline`, unless `max` beats are held.
    pub async fn wait_for_work(&self, deadline: std::time::Duration, max: usize) -> Waited {
        self.doorbell.wait_for_ring(deadline, max).await
    }

    /// The fleet event streams, bound to this store's connection.
    pub(crate) fn streams(&self) -> Handle {
        self.bus.clone()
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
