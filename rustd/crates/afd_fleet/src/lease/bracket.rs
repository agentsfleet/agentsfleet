//! The brackets the daemon puts around a run on the live tail.
//!
//! [`super::activity`] forwards the runner's mid-run frames; this module
//! publishes the two the daemon owns — `event_received` when the lease verb
//! opens the narrative log, `event_complete` when a report or a gate refusal
//! closes it. `docs/architecture/runner_fleet.md` §Live activity names them
//! the bracket frames: open and close markers that reach a watcher even for a
//! run the runner never forwarded a single frame from.
//!
//! # Best-effort, like every publish on this channel
//!
//! A frame that does not land costs the tail a marker and the run nothing.
//! The durable row is written first and stands whether or not anybody was
//! listening; the frame is the row's announcement, and a lost announcement is
//! recovered by the client's reconnect backfill from the events list. That
//! contract is [`afd_redis::FleetStreams::publish_frame`]'s, stated once for
//! every daemon-authored frame.
//!
//! # The completion is the row, not a pointer to it
//!
//! `event_complete` carries the terminal row as the events list would serve
//! it, plus the fleet's status, pending gate count, and activity counters, all
//! read by the closing statement in the same round trip that ended the run. A
//! watcher folds the frame in and issues no read — which is why the dashboard's
//! summary strip and the wall's tiles move on a completion without fetching
//! anything.
//!
//! The opening bracket cannot read its counters that way: its own insert is
//! what fires the counter trigger, and a `RETURNING` on that insert does not
//! see the trigger's write. The caller reads them after the row landed and
//! hands them in — `None` when the read did not answer, never zeros.

use std::borrow::Cow;

use afd_core::clock::UnixMillis;
use afd_events::Closed;
use afd_wire::tail::{FleetCounters, TailFrame, TailRow};

use crate::lease::envelope::Acquired;
use crate::lease::store::Leases;

impl Leases {
    /// Announce that `acquired`'s narrative log opened at `now`.
    ///
    /// Published once, on the delivery that wrote the row: a redelivery finds
    /// the row already there and a second announcement would put a duplicate
    /// marker on a tail whose row the client already holds.
    pub async fn publish_received(
        &self,
        acquired: &Acquired,
        now: UnixMillis,
        counters: Option<FleetCounters>,
    ) {
        let frame = TailFrame::EventReceived {
            event_id: Cow::Borrowed(&acquired.event_id),
            actor: Cow::Borrowed(&acquired.actor),
            event_type: Cow::Borrowed(&acquired.event_type),
            created_at: now.as_millis(),
            counters,
        };
        self.streams()
            .publish_frame(acquired.fleet_id.as_str(), &frame)
            .await;
    }

    /// Announce that a run ended, carrying the row the ending wrote.
    pub async fn publish_completion(&self, closed: &Closed) {
        let frame = TailFrame::EventComplete {
            event: Box::new(TailRow::from(closed.row.summary())),
            fleet_status: Cow::Borrowed(&closed.fleet_status),
            pending_approvals: closed.pending_approvals,
            counters: Some(closed.counters),
        };
        self.streams()
            .publish_frame(&closed.row.fleet_id, &frame)
            .await;
    }
}
