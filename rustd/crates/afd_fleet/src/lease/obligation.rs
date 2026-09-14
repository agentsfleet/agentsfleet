//! What the deployment owes a destination once a run has answered.
//!
//! Not [`super::deliver`], which is the inbound half — what an admitted claim
//! becomes when a runner takes it. This is the outbound one: the record that an
//! answer exists and has not reached anybody yet.
//!
//! # Why it is a row and not an append
//!
//! The queue append cannot join the report's transaction, because no
//! transaction spans PostgreSQL and Dragonfly. Before this module the append
//! was simply the next thing that happened after the commit, and the gap had no
//! record in it: a process dying there left a run committed, charged and
//! answered, with the answer existing nowhere. The queue never got the entry,
//! and PostgreSQL recorded only that the run had finished.
//!
//! So the obligation commits WITH the result and the append happens after, with
//! its entry id recorded back as a receipt. That is the shape
//! `core.fleet_admissions` already uses inbound, for the same reason and in the
//! same direction of safety: the durable record first, the fast path second,
//! and a scan of records that never got their receipt as the recovery set.

use afd_core::clock::UnixMillis;
use afd_core::id::{ENTROPY_LEN, Uuid7};
use afd_datastore::OutboundJob;
use sqlx::PgConnection;

use crate::error::{Result, query};
use crate::lease::sql;
use crate::lease::store::Leases;

/// Statement name, for the context a failure carries.
const CONTEXT_OWE: &str = "owe delivery";

/// Statement name, for the context a receipt failure carries.
const CONTEXT_RECEIPT: &str = "receipt delivery";

/// Everything one owed delivery is addressed by.
///
/// A struct rather than five positional `&str`-shaped parameters for the reason
/// [`TerminalReport`](crate::lease::commit::TerminalReport) is one: four of
/// these are text, and a transposition between the event and the answer — or
/// the provider and the workspace — writes a wrong row and compiles clean.
#[derive(Debug, Clone, Copy)]
pub struct Delivery<'a> {
    /// The fleet that produced the answer.
    pub fleet_id: &'a Uuid7,
    /// The workspace whose grant pays for it.
    pub workspace_id: &'a Uuid7,
    /// Which connector carries it back.
    pub provider: &'a str,
    /// The event the answer is threaded onto.
    pub event_id: &'a str,
    /// What to say.
    pub answer: &'a str,
}

impl Leases {
    /// Record that this answer is owed to its destination.
    ///
    /// Runs on the caller's connection, inside the report's transaction, so the
    /// obligation and the result it describes share one fate. A rollback that
    /// un-does the settle un-does this too, which is the property that makes
    /// "committed and charged" and "a delivery is owed" the same instant.
    ///
    /// An EMPTY answer owes nothing. A run that produced no output has nothing
    /// to say to a destination, and a row carrying an empty string would be an
    /// obligation the producer would enqueue, a poster would deliver, and a
    /// reader would see as a blank message in a real thread. The absence is
    /// deliberate rather than an oversight, so it is stated here and asserted in
    /// the suite.
    ///
    /// A repeat writes nothing: the statement conflicts on the event and does
    /// nothing, which is what keeps a re-sent report from owing the same answer
    /// to the same thread twice. The settle answers that report
    /// [`AlreadySettled`](crate::lease::settle::Settled::AlreadySettled) and
    /// charges nothing, and these two facts have to agree or an idempotent
    /// endpoint would still double-deliver.
    ///
    /// Answers the row this call created, or `None` when nothing is newly owed
    /// — an empty answer, or a repeat that conflicted. The caller appends only
    /// what it actually wrote, so a replayed report cannot put a second entry
    /// on the queue for an answer already in flight.
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
        if delivery.answer.is_empty() {
            return Ok(None);
        }

        let mut bytes = [0u8; ENTROPY_LEN];
        self.entropy().fill(&mut bytes)?;
        let row_id = Uuid7::encode(now, bytes)?;

        let written = sqlx::query(sql::report::OWE_DELIVERY)
            .bind(row_id.as_str())
            .bind(delivery.fleet_id.as_str())
            .bind(delivery.workspace_id.as_str())
            .bind(delivery.provider)
            .bind(delivery.event_id)
            .bind(delivery.answer)
            .bind(now.as_millis())
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_OWE))?;
        Ok(written.map(|_| row_id))
    }

    /// Append an owed answer to the delivery queue, then receipt the row.
    ///
    /// The second and third of the three steps, run after the transaction
    /// committed the first. Both are allowed to fail: what they leave behind is
    /// a committed obligation with no receipt, which is precisely the set the
    /// producer sweep re-appends. That is why this is the fast path and not the
    /// only path — losing it costs an answer some latency and never the answer.
    ///
    /// # Errors
    /// Reports a queue that would not accept the entry, and a datastore that
    /// would not record the receipt.
    pub async fn queue_delivery(
        &self,
        obligation: &Uuid7,
        delivery: Delivery<'_>,
        now: UnixMillis,
    ) -> Result<()> {
        let receipt = self
            .outbound()
            .enqueue(OutboundJob {
                provider: delivery.provider,
                workspace_id: delivery.workspace_id.as_str(),
                fleet_id: delivery.fleet_id.as_str(),
                event_id: delivery.event_id,
                answer: delivery.answer,
            })
            .await?;
        self.receipt_delivery(obligation, receipt.as_str(), now)
            .await
    }

    /// Record the queue entry an obligation was appended to.
    ///
    /// Runs on a pooled connection AFTER the report's transaction, because the
    /// append it records happens after that transaction too. A failure here
    /// leaves the row unreceipted, which is the recoverable state by
    /// construction: the producer's scan is exactly that set.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub async fn receipt_delivery(
        &self,
        obligation: &Uuid7,
        receipt: &str,
        now: UnixMillis,
    ) -> Result<()> {
        let mut connection = self.pool().acquire().await?;
        sqlx::query(sql::report::RECEIPT_DELIVERY)
            .bind(obligation.as_str())
            .bind(receipt)
            .bind(now.as_millis())
            .execute(&mut *connection)
            .await
            .map_err(query(CONTEXT_RECEIPT))?;
        Ok(())
    }
}
