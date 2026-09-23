//! Giving up on an answer nobody can take.
//!
//! Split from `lanes.rs` at the file-length cap, beside `retire.rs`. The lanes
//! acknowledge every terminal verdict so one undeliverable answer cannot park
//! at the head of its destination's lane; before this, acknowledging was ALL
//! they did with a refusal, so the row stayed undelivered and the producer's
//! scan re-offered it every `LOST_AFTER`, forever. Abandoning is the record
//! that it should not be.

use afd_dragonfly::OutboundDelivery;

use super::Inner;
use crate::obligation::{self, AbandonReason};
use crate::poster::Deliver;

/// Logged when an answer is given up on — once per obligation.
const EVENT_ABANDONED: &str = "outbound_delivery_abandoned";

/// Logged when the abandon stamp could not be written.
const EVENT_ABANDON_FAILED: &str = "outbound_obligation_abandon_failed";

/// What an abandonment is announced with, and all of it.
///
/// A type rather than fields spelled at the call site, so what may reach the
/// operator's log is decided here: an answer is model output and an address
/// names a person's thread, and this struct has no field that could carry
/// either.
#[derive(Debug, PartialEq, Eq)]
struct Abandoned<'a> {
    provider: &'a str,
    fleet_id: &'a str,
    reason: &'static str,
    attempt_count: i64,
}

impl<'a> Abandoned<'a> {
    /// The announcement for `job`, abandoned for `reason` after `attempt_count`.
    fn of(job: &'a OutboundDelivery, reason: AbandonReason, attempt_count: i64) -> Self {
        Self {
            provider: &job.provider,
            fleet_id: &job.fleet_id,
            reason: reason.as_str(),
            attempt_count,
        }
    }

    fn announce(&self) {
        // Hoisted: see the `tracing` note in the workspace Cargo.toml.
        let error_code = afd_core::error_code::CONNECTOR_VENDOR_DEADLINE.as_str();
        let Self {
            provider,
            fleet_id,
            reason,
            attempt_count,
        } = *self;
        tracing::warn!(
            error_code,
            provider,
            fleet_id,
            reason,
            attempt_count,
            event = EVENT_ABANDONED
        );
    }
}

impl<S: Deliver + 'static> Inner<S> {
    /// Stamps `job`'s obligation abandoned, before the acknowledgement.
    ///
    /// Announced only when THIS call stamped the row, so a duplicate entry for
    /// an answer already given up on says nothing twice. A stamp that fails is
    /// reported and the job is acknowledged anyway: the row stays in the lost
    /// set and is re-offered after the window, which is the at-least-once
    /// direction rather than a lost answer.
    pub(super) async fn abandon(&self, job: &OutboundDelivery, reason: AbandonReason) {
        match obligation::abandon(
            &self.database,
            &job.fleet_id,
            &job.event_id,
            reason,
            afd_core::clock::now(),
        )
        .await
        {
            Ok(Some(attempt_count)) => Abandoned::of(job, reason, attempt_count).announce(),
            Ok(None) => {}
            Err(failure) => crate::worker::report(EVENT_ABANDON_FAILED, &failure),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Dimension 4.4 — what an abandonment announces carries the connector, the
    /// fleet, the reason and the count, and neither the answer nor the address.
    #[test]
    fn abandonment_is_logged_once_without_content() {
        let job = OutboundDelivery {
            id: afd_dragonfly::streams::EventId::of("1700000000001-0"),
            provider: "slack".to_owned(),
            destination: r#"{"channel_id":"C0SECRET","thread_ts":"1700000000.000100"}"#.to_owned(),
            workspace_id: "0199a0b0-0000-7000-8000-000000000001".to_owned(),
            fleet_id: "0199a0b0-0000-7000-8000-000000000002".to_owned(),
            event_id: "1700000000000-0".to_owned(),
            answer: "the private answer text".to_owned(),
        };

        let announced = Abandoned::of(&job, AbandonReason::CyclesExhausted, 12);
        assert_eq!(
            announced,
            Abandoned {
                provider: "slack",
                fleet_id: "0199a0b0-0000-7000-8000-000000000002",
                reason: "cycles_exhausted",
                attempt_count: 12,
            }
        );
        let rendered = format!("{announced:?}");
        for private in [job.answer.as_str(), job.destination.as_str(), "C0SECRET"] {
            assert!(
                !rendered.contains(private),
                "the announcement carries `{private}`: {rendered}"
            );
        }
    }
}
