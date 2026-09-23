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

use afd_connector::Provider;
use afd_core::clock::UnixMillis;
use afd_core::id::{ENTROPY_LEN, Uuid7};
use afd_dragonfly::OutboundJob;
use afd_outbound::obligation::{self, Delivery};
use sqlx::PgConnection;

use crate::error::{Result, query};
use crate::lease::store::Leases;

/// Statement name, for the context a destination read failure carries.
const CONTEXT_REPLY: &str = "read reply destination";

/// Logged when a stored connector id names no connector.
const EVENT_REPLY_PROVIDER_UNKNOWN: &str = "report_reply_provider_unknown";

/// Where a settled event's answer goes, as its producer recorded it.
///
/// Parsed once, here, so everything downstream holds the connector TYPE: the
/// report also holds the lease's model provider as a string, and nothing past
/// this point can confuse the two.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplyDestination {
    /// The connector whose poster delivers the answer.
    pub provider: Provider,
    /// The address only that poster reads.
    pub address: String,
}

/// An answer this report newly owed, and where it is owed.
///
/// Carried out of the transaction to the append that follows the commit, so
/// the queue entry is addressed from what was committed rather than re-read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Owing {
    /// The obligation row this report wrote.
    pub obligation: Uuid7,
    /// Where it is owed.
    pub reply: ReplyDestination,
}

impl Leases {
    /// The destination the settled event was admitted with, or `None`.
    ///
    /// Runs on the report's own transaction, through the ledger owner's
    /// statement, so the answer describes the same snapshot the settle does.
    /// `None` covers three cases that owe the same nothing: an event id the
    /// ledger never minted, an event admitted with no destination, and a stored
    /// connector id no connector answers to — the last logged, because only an
    /// out-of-band edit or a removed connector writes one.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub(crate) async fn reply_destination(
        connection: &mut PgConnection,
        fleet_id: &str,
        event_id: &str,
    ) -> Result<Option<ReplyDestination>> {
        let Some((created_at, seq)) = afd_admission::logical_parts(event_id) else {
            return Ok(None);
        };
        let stored: Option<(String, String)> =
            sqlx::query_as(afd_admission::sql::SELECT_REPLY_DESTINATION)
                .bind(fleet_id)
                .bind(created_at)
                .bind(seq)
                .fetch_optional(&mut *connection)
                .await
                .map_err(query(CONTEXT_REPLY))?;
        Ok(stored.and_then(|(connector, address)| {
            let parsed =
                Provider::parse(&connector).map(|provider| ReplyDestination { provider, address });
            if parsed.is_none() {
                tracing::warn!(
                    fleet_id,
                    agentsfleet_event_id = event_id,
                    event = EVENT_REPLY_PROVIDER_UNKNOWN,
                    "the recorded connector names no connector; the answer is owed nowhere"
                );
            }
            parsed
        }))
    }

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
        let entry = self.outbound().enqueue(OutboundJob::from(delivery)).await?;
        obligation::receipt(self.pool(), owed.as_str(), entry.as_str(), now).await?;
        Ok(())
    }
}
