//! What a poll answers, as the bytes a runner reads.
//!
//! Split from [`super::pull`] because it is the half with no decisions in it:
//! everything here renders a conclusion someone else reached. Keeping it apart
//! is what lets the verb next door read as a sequence of decisions rather than
//! as a sequence of decisions interleaved with serialization.

use afd_core::id::Uuid7;
use afd_core::timing::NO_WORK_RETRY_AFTER_MS;
use afd_wire::event::{EventEnvelope, EventType};
use afd_wire::lease::{BundleManifest, LeasePayload, LeaseResponse, SecretDelivery};
use afd_wire::policy::ExecutionPolicy;

use crate::error::{DETAIL_EVENT_MALFORMED, Result, rejected};
use crate::lease::envelope::Acquired;
use crate::lease::installed::Installed;

/// The event a poll that issued nothing is logged under.
pub(super) const EVENT_NO_WORK: &str = "runner_lease_no_work";

/// The event an issued lease is logged under.
pub(super) const EVENT_LEASED: &str = "runner_lease_issued";

/// The event a refused delivery is logged under.
pub(super) const EVENT_REFUSED: &str = "runner_lease_refused";

/// A poll that issued nothing, logged with why.
///
/// The reason reaches the LOG and never the wire. A runner cannot act on the
/// difference between "no fleet was ready" and "a human owes an answer" — it
/// waits and asks again either way — and telling it would describe this
/// daemon's internal state to a caller with no use for it.
///
/// # Errors
/// Only a serialization failure of a fixed, field-free shape, which is
/// unreachable; it is still a `Result` so no caller has to learn that.
pub(super) fn no_work(runner_id: &Uuid7, reason: &str) -> Result<String> {
    tracing::info!(
        event = EVENT_NO_WORK,
        runner_id = runner_id.as_str(),
        reason,
        "the poll issued no lease"
    );
    answer(None)
}

/// One `LeaseResponse`, serialized.
///
/// The backoff hint rides on the EMPTY answer only: a runner handed work has
/// somewhere better to be than a sleep.
///
/// # Errors
/// Reports a payload that would not serialize.
pub(super) fn answer(lease: Option<LeasePayload<'_>>) -> Result<String> {
    let retry_after_ms = lease.is_none().then_some(NO_WORK_RETRY_AFTER_MS);
    serde_json::to_string(&LeaseResponse {
        lease,
        retry_after_ms,
    })
    .map_err(|_shape| rejected(DETAIL_EVENT_MALFORMED))
}

/// The issued lease, as the bytes a runner executes from.
///
/// # Errors
/// Reports a payload that would not serialize.
pub(super) fn render<'a>(
    lease_id: &'a Uuid7,
    acquired: &'a Acquired,
    event_type: EventType,
    installed: &'a Installed,
    policy: ExecutionPolicy<'a>,
) -> Result<String> {
    answer(Some(LeasePayload {
        lease_id: lease_id.as_str().into(),
        fencing_token: acquired.fence.as_u64(),
        lease_expires_at: acquired.leased_until.as_millis(),
        // The one delivery this port serves. `scoped` and `proxy` are wire
        // values with no implementation on either side.
        secret_delivery: SecretDelivery::Inline,
        event: EventEnvelope {
            event_id: acquired.event_id.as_str().into(),
            fleet_id: acquired.fleet_id.as_str().into(),
            workspace_id: acquired.workspace_id.as_str().into(),
            actor: acquired.actor.as_str().into(),
            event_type,
            request_json: acquired.request_json.as_str().into(),
            created_at: acquired.event_created_at.as_millis(),
        },
        policy,
        instructions: installed.instructions.as_str().into(),
        // The hash's PRESENCE is the "has bundle" signal — a fleet created
        // from no bundle carries no manifest rather than an empty one.
        bundle: installed
            .bundle_content_hash
            .as_deref()
            .map(|hash| BundleManifest {
                content_hash: hash.into(),
            }),
    }))
}
