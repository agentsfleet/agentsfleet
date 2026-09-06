//! The inbox's word on the fleet's live tail: a gate was answered.
//!
//! Split from [`super`] on the seam between deciding and announcing. The
//! resolve and the sweep move rows; this tells whoever is watching the fleet
//! that they did, and how many answers the fleet is still owed — the count
//! rides the frame so a console shows "N approvals waiting" without a read,
//! and it rode the statement that moved the row so the announcement costs no
//! read of its own either.
//!
//! # Best-effort, and what that licenses
//!
//! A publish that fails costs the tail one frame and the decision nothing:
//! the row moved before this ran, and the row is what the next poll and the
//! next page read. The dropped frame is logged by the publisher and the verb
//! answers as if it had landed.

use std::borrow::Cow;

use afd_redis::FleetStreams;
use afd_wire::tail::TailFrame;

use super::Inbox;

/// One answered gate, as the tail hears of it.
#[derive(Debug, Clone, Copy)]
pub(super) struct Answer<'a> {
    /// The fleet whose tail carries the frame.
    pub fleet_id: &'a str,
    /// The gate's row identifier.
    pub gate_id: &'a str,
    /// The event the gate held, or none for a gate raised outside a run.
    pub event_id: Option<&'a str>,
    /// Where the gate now stands.
    pub status: &'a str,
    /// Who answered — a person, or the sweeper.
    pub resolved_by: &'a str,
    /// How many of the fleet's gates still wait, as the moving statement counted.
    pub pending_approvals: i64,
}

impl Inbox {
    /// Tell the fleet's live tail a gate was answered, best-effort.
    ///
    /// After the row moved, so a watcher reacting to the frame reads the
    /// answer it names.
    pub(super) async fn announce(&self, answer: Answer<'_>) {
        let frame = TailFrame::GateResolved {
            gate_id: Cow::Borrowed(answer.gate_id),
            event_id: answer.event_id.map(Cow::Borrowed),
            status: Cow::Borrowed(answer.status),
            resolved_by: Cow::Borrowed(answer.resolved_by),
            pending_approvals: answer.pending_approvals,
        };
        FleetStreams::new(self.queue.clone())
            .publish_frame(answer.fleet_id, &frame)
            .await;
    }
}
