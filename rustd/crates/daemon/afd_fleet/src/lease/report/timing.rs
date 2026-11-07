//! A settled report's three completion stages, all on this daemon's clock.
//!
//! ```text
//!   event created ──────────────── event_to_complete ─────────────────► published
//!   lease issued ── lease_to_report ──► report received ── report_to_complete ──┘
//! ```
//!
//! Every instant is the daemon's own: the event's and the lease's creation are
//! the lease row's columns, the receipt is the verb's `now`, and the publish is
//! read when the frame went out. No runner clock is compared, so skew between
//! hosts cannot bend a sample. A report that closed no event row published
//! nothing, and records nothing.

use af_observability::metrics::label::fleet::DeliveryStage;
use af_observability::producers;
use jiff::Timestamp;

use crate::lease::settle::Reported;

/// Records the three stages of a report whose completion was published at
/// `completed_at`, received at `received`.
pub(super) fn completed(lease: &Reported, received: Timestamp, completed_at: Option<Timestamp>) {
    let Some(completed) = completed_at else {
        return;
    };
    for (stage, from, to) in stages(lease, received, completed) {
        producers::fleet::delivery_stage(stage, af_core::clock::elapsed(from, to));
    }
}

/// Each stage with the two instants it spans.
fn stages(
    lease: &Reported,
    received: Timestamp,
    completed: Timestamp,
) -> [(DeliveryStage, Timestamp, Timestamp); 3] {
    [
        (
            DeliveryStage::EventToComplete,
            lease.event_created_at,
            completed,
        ),
        (
            DeliveryStage::LeaseToReport,
            lease.lease_created_at,
            received,
        ),
        (DeliveryStage::ReportToComplete, received, completed),
    ]
}

#[cfg(all(test, feature = "test-util"))]
#[path = "timing_tests.rs"]
mod tests;
