//! Clearing a claimed fleet's readiness mark: the one write a lease poll makes
//! to the index.
//!
//! A poll clears on the two exits that prove the fleet owes nothing it can
//! deliver: a DRAINED fleet, whose group held nothing pending after the
//! takeover and nothing new, and a PARKED one, whose answer re-marks it — a
//! continuation admission, the runless wake, or the expiry sweep. Every other
//! exit keeps the mark, so the next poll comes back.
//!
//! The clear compares the generation this poll peeked inside Dragonfly, so a
//! mark ingress wrote after the peek is a newer generation and survives. It is
//! best-effort: a clear that fails leaves the mark, which costs one more empty
//! claim and never an event.

use afd_core::error_code;
use afd_core::id::Uuid7;
use afd_dragonfly::ReadyToken;
use afd_observability::producers;

use crate::lease::store::Leases;

/// A drained or parked fleet's mark would not clear.
const EVENT_READY_CLEAR_FAILED: &str = "lease_ready_clear_failed";

impl Leases {
    /// Clears `fleet_id`'s mark if it still carries `token`, the generation
    /// this poll peeked.
    ///
    /// Answers nothing: the caller's outcome is the same either way, and the
    /// failure is counted and logged here.
    pub(crate) async fn clear_mark(&self, fleet_id: &Uuid7, token: &ReadyToken) {
        let fleet = fleet_id.as_str();
        if let Err(failure) = self.ready().clear_if_unchanged(fleet, token).await {
            producers::fleet::ready_write_failed();
            let code = error_code::INTERNAL_OPERATION_FAILED.as_str();
            let reason = failure.to_string();
            tracing::warn!(
                error_code = code,
                event = EVENT_READY_CLEAR_FAILED,
                fleet_id = fleet,
                reason,
                "a fleet with nothing deliverable kept its readiness mark; the next poll claims it once more"
            );
        }
    }
}

#[cfg(all(test, feature = "test-util"))]
mod tests {
    use afd_core::error_code;
    use afd_observability::test_util::Capture;

    use super::EVENT_READY_CLEAR_FAILED;
    use crate::lease::{test_dead, test_log::Recorder};

    /// The family a refused clear is counted under, with every failed mark.
    const READY_WRITE_FAILURES: &str = "agentsfleet_fleet_ready_write_failures_total";

    /// A clear the index refuses leaves the caller's answer alone, and is
    /// counted and logged — the mark stays, which costs one more empty claim.
    #[tokio::test]
    async fn should_count_and_log_a_clear_the_index_refuses() {
        let capture = Capture::install();
        let log = Recorder::install();
        let acquired = test_dead::acquired();
        let before = capture.sum(READY_WRITE_FAILURES, &[]);

        test_dead::leases()
            .clear_mark(&acquired.fleet_id, &acquired.ready)
            .await;

        let after = capture.sum(READY_WRITE_FAILURES, &[]);
        assert!(
            after > before,
            "the refused clear is counted: {before} -> {after}"
        );
        let line = log.only(EVENT_READY_CLEAR_FAILED);
        assert_eq!(
            line.get("error_code").map(String::as_str),
            Some(error_code::INTERNAL_OPERATION_FAILED.as_str())
        );
        assert_eq!(
            line.get("fleet_id").map(String::as_str),
            Some(acquired.fleet_id.as_str())
        );
    }
}
