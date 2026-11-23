//! What a holder does with its claim while it delivers: renew it, give it
//! back at shutdown, mark a failed delivery, or give it up when another holder
//! has it.

use std::time::Duration;

use super::{
    EVENT_CLAIM_LAPSED, EVENT_DELIVERY_EXHAUSTED, EVENT_RELEASE_FAILED, EVENT_RENEW_FAILED,
    EVENT_REQUEUED_AT_SHUTDOWN, Inner,
};
use crate::delivery::{self, AbandonReason, CLAIM_TTL, Claimed};
use crate::poster::Deliver;

impl<S: Deliver + 'static> Inner<S> {
    /// Renews `job`'s claim until it is lost, and answers how: `renew` when
    /// another holder took the row, `renew_unanswered` when no renewal has
    /// landed for so long that the claim may lapse before the next one.
    /// Never returns while the claim is known to hold.
    pub(super) async fn renewing(&self, job: &Claimed) -> &'static str {
        hold_claim(self.renew, || delivery::renew(&self.database, job)).await
    }

    /// A cycle whose every inline attempt ended retryably: the delivery is
    /// marked failed and never offered again. A shutdown instead gives the
    /// claim back, so the interrupted delivery is not counted as failed.
    pub(super) async fn fail(&self, job: &Claimed, attempts: Option<i64>) {
        let provider = job.outgoing.provider.as_str();
        let fleet_id = job.outgoing.fleet_id.as_str();
        if self.token.is_cancelled() {
            {
                let event = &EVENT_REQUEUED_AT_SHUTDOWN;
                tracing::info!(provider, fleet_id, attempts, event);
            };
            self.release(job).await;
            return;
        }
        let error_code = af_core::error_code::CONNECTOR_VENDOR_DEADLINE.as_str();
        {
            let event = &EVENT_DELIVERY_EXHAUSTED;
            tracing::warn!(error_code, provider, fleet_id, attempts, event);
        };
        crate::abandon::delivery(&self.database, job, AbandonReason::AttemptsExhausted).await;
    }

    /// Gives `job`'s claim back, claimable at once.
    pub(super) async fn release(&self, job: &Claimed) {
        match delivery::release(&self.database, job).await {
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
        {
            let event = &EVENT_CLAIM_LAPSED;
            tracing::warn!(error_code, provider, fleet_id, write, event);
        };
    }
}

/// Renews through `renew` every `every` until the claim is lost, and answers
/// how, as [`Inner::renewing`] describes.
///
/// The claim is proved held only from the moment a renewal that landed was
/// sent. Each renewal races a deadline half an interval before the claim could
/// lapse, so a database that stops answering mid-call cannot keep a delivery
/// posting past the point another replica may retake the row.
async fn hold_claim<F, Fut>(every: Duration, mut renew: F) -> &'static str
where
    F: FnMut() -> Fut,
    Fut: Future<Output = crate::error::Result<bool>>,
{
    let mut held_at = tokio::time::Instant::now();
    loop {
        tokio::time::sleep(every).await;
        let sent = tokio::time::Instant::now();
        let give_up_at = held_at + CLAIM_TTL.saturating_sub(every / 2);
        match tokio::time::timeout_at(give_up_at, renew()).await {
            Err(_unanswered) => return "renew_unanswered",
            Ok(Ok(true)) => held_at = sent,
            Ok(Ok(false)) => return "renew",
            Ok(Err(failure)) => {
                crate::worker::report(EVENT_RENEW_FAILED, &failure);
                // Stop before the claim can lapse: a replica may retake
                // the row, and two holders would both post it.
                if held_at.elapsed() + every >= CLAIM_TTL {
                    return "renew_unanswered";
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "claim_tests.rs"]
mod tests;
