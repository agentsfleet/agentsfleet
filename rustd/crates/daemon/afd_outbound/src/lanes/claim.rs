//! What a holder does with its claim while it delivers: renew it, give it
//! back due later, or give it up when another holder has it.

use std::time::Duration;

use super::{
    CYCLE_BACKOFF, EVENT_CLAIM_LAPSED, EVENT_DELIVERY_EXHAUSTED, EVENT_RELEASE_FAILED,
    EVENT_RENEW_FAILED, EVENT_REQUEUED_AT_SHUTDOWN, Inner, MAX_DELIVERY_CYCLES,
};
use crate::obligation::{self, AbandonReason, CLAIM_TTL, Claimed};
use crate::poster::Deliver;

impl<S: Deliver + 'static> Inner<S> {
    /// Renews `job`'s claim until it is lost, and answers how: `renew` when
    /// another holder took the row, `renew_unanswered` when no renewal has
    /// landed for so long that the claim may lapse before the next one.
    /// Never returns while the claim is known to hold.
    pub(super) async fn renewing(&self, job: &Claimed) -> &'static str {
        let mut held_at = tokio::time::Instant::now();
        loop {
            tokio::time::sleep(self.renew).await;
            match obligation::renew(&self.database, job).await {
                Ok(true) => held_at = tokio::time::Instant::now(),
                Ok(false) => return "renew",
                Err(failure) => {
                    crate::worker::report(EVENT_RENEW_FAILED, &failure);
                    // Stop before the claim can lapse: a replica may retake
                    // the row, and two holders would both post it.
                    if held_at.elapsed() + self.renew >= CLAIM_TTL {
                        return "renew_unanswered";
                    }
                }
            }
        }
    }

    /// A retryable cycle: give the claim back, due again later, or abandon it
    /// once its cycles are spent. A shutdown gives it back due at once.
    pub(super) async fn retry_later(&self, job: &Claimed, attempts: Option<i64>) {
        let provider = job.outgoing.provider.as_str();
        let fleet_id = job.outgoing.fleet_id.as_str();
        if self.token.is_cancelled() {
            tracing::info!(
                provider,
                fleet_id,
                attempts,
                event = EVENT_REQUEUED_AT_SHUTDOWN
            );
            self.release(job, Duration::ZERO).await;
            return;
        }
        let error_code = af_core::error_code::CONNECTOR_VENDOR_DEADLINE.as_str();
        tracing::warn!(
            error_code,
            provider,
            fleet_id,
            attempts,
            event = EVENT_DELIVERY_EXHAUSTED
        );
        if attempts.is_some_and(|cycles| cycles >= MAX_DELIVERY_CYCLES) {
            crate::abandon::delivery(&self.database, job, AbandonReason::CyclesExhausted).await;
            return;
        }
        self.release(job, CYCLE_BACKOFF).await;
    }

    pub(super) async fn release(&self, job: &Claimed, after: Duration) {
        match obligation::release(&self.database, job, after).await {
            Ok(true) => {}
            Ok(false) => Self::lapsed(job, "release"),
            Err(failure) => crate::worker::report(EVENT_RELEASE_FAILED, &failure),
        }
    }

    /// A holder's write matched nothing: its claim had lapsed.
    pub(super) fn lapsed(job: &Claimed, write: &'static str) {
        let provider = job.outgoing.provider.as_str();
        let fleet_id = job.outgoing.fleet_id.as_str();
        let error_code = af_core::error_code::INTERNAL_OPERATION_FAILED.as_str();
        tracing::warn!(
            error_code,
            provider,
            fleet_id,
            write,
            event = EVENT_CLAIM_LAPSED
        );
    }
}
