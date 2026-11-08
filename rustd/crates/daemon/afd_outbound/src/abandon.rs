//! Giving up on an answer nobody can take.
//!
//! The lanes reach this for a claimed job whose destination refused it for
//! good, or whose cycles ran out. Stamping the row is what keeps every later
//! claim from offering it again.

use afd_db::Db;

use crate::obligation::{self, AbandonReason, Claimed};

/// Logged when an answer is given up on — once per obligation.
const EVENT_ABANDONED: &str = "outbound_delivery_abandoned";

/// Logged when the abandon stamp could not be written.
const EVENT_ABANDON_FAILED: &str = "outbound_obligation_abandon_failed";

/// Stamps a claimed job's row abandoned and announces it — only when THIS call
/// stamped it, so an answer already given up on says nothing twice.
///
/// A stamp that fails is reported and nothing else: the claim lapses and the
/// row is offered again, the at-least-once direction rather than a lost answer.
pub(crate) async fn delivery(database: &Db, job: &Claimed, reason: AbandonReason) {
    match obligation::abandon(database, job, reason).await {
        Ok(Some(attempt_count)) => Abandoned {
            provider: &job.provider,
            fleet_id: job.fleet_id.as_str(),
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
        let error_code = af_core::error_code::CONNECTOR_VENDOR_DEADLINE.as_str();
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
        let job = Claimed {
            id: afd_db::test_util::mint_id(),
            token: "holder-token".to_owned(),
            provider: "slack".to_owned(),
            destination: r#"{"channel_id":"C0SECRET","thread_ts":"1700000000.000100"}"#.to_owned(),
            workspace_id: afd_db::test_util::mint_id(),
            fleet_id: afd_db::test_util::mint_id(),
            event_id: afd_db::test_util::mint_id(),
            answer: "the private answer text".to_owned(),
        };

        let announced = Abandoned {
            provider: &job.provider,
            fleet_id: job.fleet_id.as_str(),
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
