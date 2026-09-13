//! Reading a claimed fleet's next entry, and restoring its consumer group
//! when the stream says there is none.
//!
//! # Why the restore lives here and not in the datastore
//!
//! `afd_datastore` reports a vanished group and refuses to guess where to
//! recreate it, because the two blind positions are both wrong: `0` re-runs
//! every retained entry — the lease path re-executes a redelivered entry,
//! merely skipping its receive debit — and `$` loses everything appended
//! while the group was gone. The position is a fact about what RAN, which
//! only the durable ledgers know, and this is the one place that holds both
//! the ledgers and the stream.
//!
//! # Once, and then the read is tried again
//!
//! A second `NOGROUP` after a restore is reported, not retried: something is
//! destroying the group faster than it is recreated, and looping here would
//! turn a poll into a spin against a datastore that is already misbehaving.

use afd_core::error_code;
use afd_datastore::{FleetEvent, FleetStreams};

use crate::error::Result;
use crate::lease::assign::warn_queue_fleet;
use crate::lease::store::Leases;

/// The consumer's own pending list would not answer.
const EVENT_PEL_READ_FAILED: &str = "assign_pel_read_failed";

/// The fleet stream would not answer.
const EVENT_STREAM_READ_FAILED: &str = "assign_xreadgroup_failed";

/// An entry this consumer already held came back.
const EVENT_PEL_REDELIVERED: &str = "assign_pel_redelivered";

/// A vanished consumer group was recreated at the ledgers' cursor.
const EVENT_GROUP_RESTORED: &str = "fleet_consumer_group_restored";

impl Leases {
    /// This consumer's own pending entry first, then a new one — restoring
    /// the fleet's consumer group at the ledgers' cursor, once, if the
    /// stream has lost it.
    ///
    /// # Errors
    /// Reports a queue that would not answer, and a ledger that could not
    /// say where to resume. Both are logged here, because the lease poll
    /// propagates them and the handler's line would otherwise be the only
    /// record.
    pub(super) async fn read_fresh(
        &self,
        fleet: &str,
        consumer: &str,
    ) -> Result<Option<FleetEvent>> {
        let streams = self.streams();
        let read = match pending_then_new(&streams, fleet, consumer).await {
            Err(lost) if lost.is_group_missing() => {
                let cursor = self.admissions().delivered_cursor(fleet).await?;
                // Hoisted: see the `tracing` note in the workspace Cargo.toml.
                let code = error_code::INTERNAL_OPERATION_FAILED.as_str();
                let resumed_at = format!("{cursor:?}");
                tracing::warn!(
                    error_code = code,
                    fleet_id = fleet,
                    resumed_at,
                    event = EVENT_GROUP_RESTORED,
                    "the fleet's consumer group was gone and was recreated where the ledgers say delivery stopped"
                );
                streams.restore_group(fleet, &cursor).await?;
                pending_then_new(&streams, fleet, consumer).await
            }
            other => other,
        };
        Ok(read?)
    }
}

/// The two reads, in the order that keeps a re-poll ahead of new work.
///
/// A failed pending read cannot PROVE the pending list is empty, so it must
/// not fall through to the fresh read — promoting a new entry over a
/// possibly-pending re-poll would break own-pending-first ordering exactly
/// when the queue is degraded. Propagating is what stops it.
async fn pending_then_new(
    streams: &FleetStreams,
    fleet: &str,
    consumer: &str,
) -> afd_datastore::error::Result<Option<FleetEvent>> {
    let pending = streams
        .read_pending(fleet, consumer)
        .await
        .inspect_err(|error| warn_queue_fleet(EVENT_PEL_READ_FAILED, fleet, error))?;
    match pending {
        Some(event) => {
            let id = event.receipt.as_str();
            tracing::debug!(
                event = EVENT_PEL_REDELIVERED,
                fleet_id = fleet,
                receipt = id,
                "an entry this consumer already held is being re-delivered"
            );
            Ok(Some(event))
        }
        None => streams
            .read_new(fleet, consumer)
            .await
            .inspect_err(|error| warn_queue_fleet(EVENT_STREAM_READ_FAILED, fleet, error)),
    }
}
