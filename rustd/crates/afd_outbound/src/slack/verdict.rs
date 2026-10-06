//! What a Slack answer is worth: the verdict a status and a body earn, and the
//! line a failure is logged as.
//!
//! Split from [`super`], which holds the poster, so the vendor call and the
//! classification of its answer read apart. Nothing here makes a request.

use afd_connector::slack::Thread;
use afd_dragonfly::OutboundDelivery;

use super::{
    Accepted, REASON_ADDRESS_UNREADABLE, STATUS_OK, STATUS_SERVER_ERROR_FLOOR,
    STATUS_TOO_MANY_REQUESTS,
};
use crate::poster::Verdict;

/// Where the answer goes, read from the job's recorded address.
///
/// Not JSON, missing a field, or naming an empty channel or thread — one
/// answer for all of them, because a caller does the same thing with each: the
/// job names nowhere this poster can post, and no retry changes that. Nothing
/// has been read or requested when it answers.
pub(super) fn destination(job: &OutboundDelivery) -> Result<Thread, Verdict> {
    Thread::parse(&job.destination)
        .ok_or_else(|| failed(job, REASON_ADDRESS_UNREADABLE, Verdict::Permanent))
}

/// The verdict a status and a body earn, or the event a failure is logged as.
///
/// `Ok` for the one success. `Err` carries the event name, which the caller
/// pairs with [`verdict_of`]. Split because the two are different facts: the
/// verdict decides what happens next, and the event is what an operator greps
/// — and §8A asks a port to keep the Zig's event spellings, which a verdict
/// enum has no room to carry.
pub(super) fn classify(status: u16, payload: &str) -> Result<Verdict, &'static str> {
    if status == STATUS_TOO_MANY_REQUESTS || status >= STATUS_SERVER_ERROR_FLOOR {
        return Err("slack_post_retryable");
    }
    if status != STATUS_OK {
        return Err("slack_post_unexpected_status");
    }
    // Slack answers 200 with `{"ok": false}` for app-level refusals — a channel
    // that is gone, a scope that was never granted. The status alone would read
    // every one of those as a delivered answer.
    if serde_json::from_str::<Accepted>(payload).is_ok_and(|body| body.ok) {
        Ok(Verdict::Delivered)
    } else {
        Err("slack_post_app_error")
    }
}

/// The verdict a status earns once [`classify`] has refused it.
pub(super) const fn verdict_of(status: u16) -> Verdict {
    if status == STATUS_TOO_MANY_REQUESTS || status >= STATUS_SERVER_ERROR_FLOOR {
        Verdict::Retryable
    } else {
        // Includes the 200 that carried `{"ok": false}`: a bad scope or a
        // deleted channel refuses identically on every retry.
        Verdict::Permanent
    }
}

/// Logs why a delivery did not land and returns the verdict it earns.
///
/// The event goes to the operator and never to Slack. One site, so a failure
/// added later cannot be the one that forgets to say anything.
pub(super) fn failed(job: &OutboundDelivery, event: &'static str, verdict: Verdict) -> Verdict {
    // Hoisted: see the `tracing` note in the workspace Cargo.toml.
    let error_code = afd_core::error_code::CONNECTOR_VENDOR_DEADLINE.as_str();
    let workspace_id = job.workspace_id.as_str();
    let fleet_id = job.fleet_id.as_str();
    let reason = match verdict {
        Verdict::Delivered | Verdict::Permanent => "permanent",
        Verdict::Retryable => "retryable",
    };
    tracing::warn!(error_code, workspace_id, fleet_id, reason, event);
    verdict
}
