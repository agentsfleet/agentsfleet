//! Reading a claimed fleet's next entry, and restoring its consumer group
//! when the stream says there is none.
//!
//! # The oldest owed entry, whichever consumer holds it
//!
//! A won claim proves no live lease holds the fleet, so any entry pending in
//! the group is owed rather than in flight: this process's own re-poll, a
//! parked event, or one another replica read before it died. The read takes
//! the group's oldest pending entry over into this consumer first, and reads
//! a new one only when nothing is pending anywhere. Reading this consumer's
//! own list alone would miss the third case, and a poll that then found
//! nothing new would clear the fleet's mark over an entry no process reads.
//!
//! # Why the restore lives here and not in the datastore
//!
//! `afd_dragonfly` reports a vanished group and refuses to guess where to
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
use afd_dragonfly::{FleetEvent, FleetStreams};

use crate::error::Result;
use crate::lease::assign::warn_queue_fleet;
use crate::lease::store::Leases;

/// The group's pending list would not answer the takeover.
const EVENT_PEL_READ_FAILED: &str = "assign_pel_read_failed";

/// The fleet stream would not answer.
const EVENT_STREAM_READ_FAILED: &str = "assign_xreadgroup_failed";

/// An entry pending in the group was taken over and is being re-delivered.
const EVENT_PEL_REDELIVERED: &str = "assign_pel_redelivered";

/// A vanished consumer group was recreated at the ledgers' cursor.
const EVENT_GROUP_RESTORED: &str = "fleet_consumer_group_restored";

impl Leases {
    /// The group's oldest pending entry, taken over into `consumer`, then a
    /// new one — restoring the fleet's consumer group at the ledgers' cursor,
    /// once, if the stream has lost it.
    ///
    /// `None` is the one proof a poll has that the fleet is drained: nothing
    /// pending anywhere in the group, and nothing new.
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
        let read = match owed_then_new(&streams, fleet, consumer).await {
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
                owed_then_new(&streams, fleet, consumer).await
            }
            other => other,
        };
        Ok(read?)
    }
}

/// The two reads, in the order that keeps owed work ahead of new work.
///
/// A failed takeover cannot PROVE nothing is pending, so it must not fall
/// through to the fresh read — promoting a new entry over a possibly-pending
/// one would break the fleet's order exactly when the queue is degraded, and
/// an empty answer from there would clear a mark over owed work. Propagating
/// is what stops both.
async fn owed_then_new(
    streams: &FleetStreams,
    fleet: &str,
    consumer: &str,
) -> afd_dragonfly::error::Result<Option<FleetEvent>> {
    let pending = streams
        .take_over_oldest(fleet, consumer)
        .await
        .inspect_err(|error| warn_queue_fleet(EVENT_PEL_READ_FAILED, fleet, error))?;
    match pending {
        Some(event) => {
            let id = event.receipt.as_str();
            tracing::debug!(
                event = EVENT_PEL_REDELIVERED,
                fleet_id = fleet,
                receipt = id,
                "an entry pending in the group was taken over and is being re-delivered"
            );
            Ok(Some(event))
        }
        None => streams
            .read_new(fleet, consumer)
            .await
            .inspect_err(|error| warn_queue_fleet(EVENT_STREAM_READ_FAILED, fleet, error)),
    }
}
