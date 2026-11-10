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
use afd_bus::{Handle, wake::Wake};
use afd_db::Db;

/// Queue selection, lease writes and owed answers use Postgres. The bus
/// carries the live tail and the disposable notifications.
#[derive(Debug, Clone)]
pub struct Leases {
    database: Db,
    bus: Handle,
    entropy: Entropy,
    wake: Wake,
    ticket: Option<afd_runner::capacity::choice::Ticket>,
    authority: Option<afd_runner::capacity::authority::Authority>,
    /// Parsed definitions by revision id, shared by every clone of this store.
    definitions: crate::lease::cache::Definitions,
    /// Price books by model and catalogue version, shared the same way.
    prices: crate::lease::cache::PriceBooks,
}

impl Leases {
    /// A store reading and writing through `database`, announcing on `bus`.
    #[must_use]
    pub fn new(database: Db, bus: Handle, entropy: Entropy) -> Self {
        Self {
            database,
            bus,
            entropy,
            wake: Wake::default(),
            ticket: None,
            authority: None,
            definitions: crate::lease::cache::Definitions::default(),
            prices: crate::lease::cache::PriceBooks::default(),
        }
    }

    /// The price books this replica holds, by model and catalogue version.
    pub(crate) const fn prices(&self) -> &crate::lease::cache::PriceBooks {
        &self.prices
    }

    /// The parsed definitions this replica holds, by revision id.
    pub(crate) const fn definitions(&self) -> &crate::lease::cache::Definitions {
        &self.definitions
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

    /// The connection's proof for `runner`: its credential digest and
    /// connection, or none for a trusted domain call.
    pub(crate) fn host_proof<'a>(
        &'a self,
        runner: &'a af_core::id::Uuid7,
    ) -> (Option<&'a str>, Option<&'a af_core::id::Uuid7>) {
        let (_runner, digest, connection) = match &self.authority {
            Some(authority) => afd_runner::capacity::proof((runner, authority)),
            None => afd_runner::capacity::proof(runner),
        };
        (digest, connection)
    }

    /// Lock the runner row and prove its owner, leaving the census to the
    /// fleet issue's guarded insert (M245 §5).
    pub(crate) async fn lock_host(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        runner: &af_core::id::Uuid7,
    ) -> crate::Result<Option<afd_runner::capacity::Held>> {
        Ok(match &self.authority {
            Some(authority) => afd_runner::capacity::lock_host(tx, (runner, authority)).await?,
            None => afd_runner::capacity::lock_host(tx, runner).await?,
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

    /// Share the replica's notification queue with placement and scheduling.
    #[must_use]
    pub fn with_wake(mut self, wake: Wake) -> Self {
        self.wake = wake;
        self
    }

    /// The entropy source, for the sibling module that mints a lease's
    /// identifiers.
    ///
    /// `pub(crate)` for the same reason [`Leases::pool`] is.
    pub(crate) const fn entropy(&self) -> &Entropy {
        &self.entropy
    }

    /// Recent-work reminders supplement the authoritative candidate read.
    #[must_use]
    pub const fn wake(&self) -> &Wake {
        &self.wake
    }

    /// The fleet event streams, bound to this store's connection.
    pub(crate) fn streams(&self) -> Handle {
        self.bus.clone()
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
