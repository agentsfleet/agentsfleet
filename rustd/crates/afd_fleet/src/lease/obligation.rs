//! Owing a delivery from inside the report's transaction.
//!
//! The table itself lives in [`afd_outbound::obligation`], which owns every
//! statement that touches `core.fleet_obligations`. This module is the seam: it
//! mints the row's identifier from this store's entropy, calls the owner's verb
//! on the transaction's own connection, and hands the queue append back to the
//! post-commit path.
//!
//! Not [`super::deliver`], which is the inbound half — what an admitted claim
//! becomes when a runner takes it. This is the outbound one: the record that an
//! answer exists and has not reached anybody yet.
//!
//! # Why the statements are NOT here
//!
//! They were, briefly, and the split immediately produced two `UPDATE … SET
//! receipt` statements differing by one guard and two structs for the same six
//! columns — one on each side of the crate boundary. `afd_events::sql` carries
//! the same lesson in its own note. The owner is the crate that DELIVERS; this
//! one commits the obligation and then gets out of the way.

use afd_core::clock::UnixMillis;
use afd_core::id::{ENTROPY_LEN, Uuid7};
use afd_datastore::OutboundJob;
use afd_outbound::obligation::{self, Delivery};
use sqlx::PgConnection;

use crate::error::Result;
use crate::lease::store::Leases;

impl Leases {
    /// Record that this answer is owed to its destination.
    ///
    /// Runs on the caller's connection, inside the report's transaction, so the
    /// obligation and the result it describes share one fate. A rollback that
    /// un-does the settle un-does this too, which is the property that makes
    /// "committed and charged" and "a delivery is owed" the same instant.
    ///
    /// Answers the row this call created, or `None` when nothing is newly owed
    /// — an empty answer, or a repeat that conflicted. The caller appends only
    /// what it actually wrote, so a replayed report cannot put a second entry on
    /// the queue for an answer already in flight.
    ///
    /// # Errors
    /// Reports an entropy source that could not produce the row's identifier,
    /// an instant that cannot be encoded, and a datastore that would not answer.
    pub async fn owe_delivery(
        &self,
        connection: &mut PgConnection,
        delivery: Delivery<'_>,
        now: UnixMillis,
    ) -> Result<Option<Uuid7>> {
        let mut bytes = [0u8; ENTROPY_LEN];
        self.entropy().fill(&mut bytes)?;
        let row_id = Uuid7::encode(now, bytes)?;

        let written = obligation::owe(connection, row_id.as_str(), delivery, now).await?;
        Ok(written.then_some(row_id))
    }

    /// Append an owed answer to the delivery queue, then receipt the row.
    ///
    /// The second and third of the three steps, run after the transaction
    /// committed the first. Both are allowed to fail: what they leave behind is
    /// a committed obligation with no receipt, which is precisely the set the
    /// producer re-appends. That is why this is the fast path and not the only
    /// path — losing it costs an answer some latency and never the answer.
    ///
    /// # Errors
    /// Reports a queue that would not accept the entry, and a datastore that
    /// would not record the receipt.
    pub async fn queue_delivery(
        &self,
        owed: &Uuid7,
        delivery: Delivery<'_>,
        now: UnixMillis,
    ) -> Result<()> {
        let entry = self
            .outbound()
            .enqueue(OutboundJob {
                provider: delivery.provider,
                workspace_id: delivery.workspace_id,
                fleet_id: delivery.fleet_id,
                event_id: delivery.event_id,
                answer: delivery.answer,
            })
            .await?;
        obligation::receipt(self.pool(), owed.as_str(), entry.as_str(), now).await?;
        Ok(())
    }
}
