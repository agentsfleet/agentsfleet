//! Giving up on an answer nobody can take.
//!
//! Two callers reach the same verdict from two directions. The lanes hold a
//! queued job whose destination refused it for good, or whose cycles ran out;
//! acknowledging was once ALL they did with such a job, so the row stayed
//! undelivered and the producer's scan re-offered it every `LOST_AFTER`,
//! forever. The producer holds a scanned row whose stored connector no
//! connector answers to; skipping it left it in every scan, and a batch of
//! them fills `BATCH_LIMIT` and starves every answer queued behind it. Both
//! stamp the row here, so no scan offers it again.

use afd_db::Db;
use afd_dragonfly::OutboundDelivery;

use crate::obligation::{self, AbandonReason, Owed};

/// Logged when an answer is given up on — once per obligation.
const EVENT_ABANDONED: &str = "outbound_delivery_abandoned";

/// Logged when the abandon stamp could not be written.
const EVENT_ABANDON_FAILED: &str = "outbound_obligation_abandon_failed";

/// Abandons a queued job's obligation, before its acknowledgement.
pub(crate) async fn delivery(database: &Db, job: &OutboundDelivery, reason: AbandonReason) {
    stamp(
        database,
        &job.provider,
        &job.fleet_id,
        &job.event_id,
        reason,
    )
    .await;
}

/// Abandons a scanned row no queue entry could deliver.
pub(crate) async fn row(database: &Db, owed: &Owed, reason: AbandonReason) {
    stamp(
        database,
        &owed.provider,
        &owed.fleet_id,
        &owed.event_id,
        reason,
    )
    .await;
}

/// Stamps the row and announces it — only when THIS call stamped it, so a
/// duplicate entry for an answer already given up on says nothing twice.
///
/// A stamp that fails is reported and nothing else: the caller acknowledges or
/// moves on, and the row stays in the lost set to be offered again after the
/// window — the at-least-once direction rather than a lost answer.
async fn stamp(
    database: &Db,
    provider: &str,
    fleet_id: &str,
    event_id: &str,
    reason: AbandonReason,
) {
    match obligation::abandon(database, fleet_id, event_id, reason, afd_core::clock::now()).await {
        Ok(Some(attempt_count)) => Abandoned {
            provider,
            fleet_id,
            reason: reason.as_str(),
            attempt_count,
        }
        .announce(),
        Ok(None) => {}
        Err(failure) => crate::worker::report(EVENT_ABANDON_FAILED, &failure),
    }
}

/// What an abandonment is announced with, and all of it.
///
/// A type rather than fields spelled at each call site, so what may reach the
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

impl Abandoned<'_> {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Dimension 4.4 — what an abandonment announces carries the connector, the
    /// fleet, the reason and the count, and neither the answer nor the address.
    ///
    /// The job below carries both, and the announcement is built from the same
    /// fields the lanes build it from; its type has nowhere to put either.
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

        let announced = Abandoned {
            provider: &job.provider,
            fleet_id: &job.fleet_id,
            reason: AbandonReason::CyclesExhausted.as_str(),
            attempt_count: 12,
        };
        let rendered = format!("{announced:?}");
        for private in [job.answer.as_str(), job.destination.as_str(), "C0SECRET"] {
            assert!(
                !rendered.contains(private),
                "the announcement carries `{private}`: {rendered}"
            );
        }
        assert!(rendered.contains("cycles_exhausted") && rendered.contains("12"));
    }

    /// Every reason has its own stored spelling, so an operator filtering on
    /// one never collects another.
    #[test]
    fn every_abandon_reason_spells_itself_apart() {
        let spellings = [
            AbandonReason::Refused.as_str(),
            AbandonReason::CyclesExhausted.as_str(),
            AbandonReason::Unaddressable.as_str(),
        ];
        let distinct: std::collections::BTreeSet<&str> = spellings.into_iter().collect();
        assert_eq!(distinct.len(), spellings.len(), "{spellings:?}");
    }
}
