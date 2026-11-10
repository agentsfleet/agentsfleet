//! Owing a delivery from inside the report's transaction.
//!
//! The table itself lives in [`afd_outbound::obligation`], which owns every
//! statement that touches `fleet.obligations`. This module is the seam: it
//! mints the row's identifier from this store's entropy, calls the owner's verb
//! on the transaction's own connection, and hands the new row's identifier back
//! so the post-commit path can ring the outbound workers' wake.
//!
//! Not `super::deliver`, which is the inbound half — what an admitted claim
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

use af_core::id::Uuid7;
use afd_connector::Provider;
use afd_outbound::obligation::{self, Delivery};
use jiff::Timestamp;
use sqlx::PgConnection;

use crate::error::{Result, query};
use crate::lease::store::Leases;

/// Statement name, for the context a destination read failure carries.
const CONTEXT_REPLY: &str = "read reply destination";

/// Logged when a stored connector id names no connector.
const EVENT_REPLY_PROVIDER_UNKNOWN: &str = "report_reply_provider_unknown";

/// Logged when the bus refuses the outbound workers' wake.
pub(crate) const EVENT_OUTBOUND_WAKE_LOST: &str = "outbound_wake_lost";

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

/// An answer this report newly owed.
///
/// Carried out of the transaction to the wake that follows the commit. The
/// wake names only the row: a worker claims it and reads where it is owed
/// from what was committed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Owing {
    /// The obligation row this report wrote.
    pub obligation: Uuid7,
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
        fleet_id: &Uuid7,
        event_id: &str,
    ) -> Result<Option<ReplyDestination>> {
        let Ok(admission) = Uuid7::parse(event_id) else {
            return Ok(None);
        };
        let stored: Option<(String, String)> =
            sqlx::query_as(afd_admission::sql::SELECT_REPLY_DESTINATION)
                .bind(fleet_id)
                .bind(&admission)
                .fetch_optional(&mut *connection)
                .await
                .map_err(query(CONTEXT_REPLY))?;
        Ok(stored.and_then(|(connector, address)| {
            let parsed =
                Provider::parse(&connector).map(|provider| ReplyDestination { provider, address });
            if parsed.is_none() {
                tracing::warn!(
                    fleet_id = %fleet_id,
                    agentsfleet_event_id = %event_id,
                    event = %EVENT_REPLY_PROVIDER_UNKNOWN,
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
    /// — an empty answer, or a repeat that conflicted. The caller wakes the
    /// outbound workers only for what it actually wrote, so a replayed report
    /// cannot ring for an answer already in flight.
    ///
    /// # Errors
    /// Reports an entropy source that could not produce the row's identifier,
    /// an instant that cannot be encoded, and a datastore that would not answer.
    pub async fn owe_delivery(
        &self,
        connection: &mut PgConnection,
        delivery: Delivery<'_>,
        now: Timestamp,
    ) -> Result<Option<Uuid7>> {
        let row_id = self.entropy().uuid7(now)?;
        // The report's own token, so no holder's write matches it until a
        // claim replaces it.
        let token = self.entropy().uuid7(now)?;
        let written = obligation::owe(connection, &row_id, &token, delivery).await?;
        Ok(written.then_some(row_id))
    }

    /// Ping the outbound workers that an owed answer committed.
    ///
    /// At most once: a lost ping costs the answer up to one claim pass of
    /// latency and never the answer, because the row is free to claim the
    /// moment its report committed. So a refusal is logged here, once, and
    /// never returned: no caller could do more with it.
    pub async fn wake_delivery(&self, owed: &Uuid7) {
        if let Err(error) = self
            .streams()
            .publish(&afd_bus::Subject::OutboundWake, owed.as_str())
            .await
        {
            let error_code = error.code().as_str();
            let reason = error.reason();
            tracing::warn!(
                event = EVENT_OUTBOUND_WAKE_LOST,
                error_code,
                reason,
                "the answer waits for the next claim pass"
            );
        }
    }
}

#[cfg(all(test, feature = "test-util"))]
#[path = "obligation_tests.rs"]
mod tests;
