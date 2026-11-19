use af_core::{error_code::Coded as _, id::Uuid7, timing::READY_WAKE_DEADLINE};
use af_observability::{metrics::label::fleet::ReadyWake, producers};

use crate::{Handle, Subject};

/// Publish only after commit; failure cannot reject work already accepted.
///
/// `owner` is the waiting work's thread, a fleet or a setup draft, named by
/// its own id: one wake subject serves both kinds.
pub async fn publish(bus: &Handle, owner: Uuid7) {
    let payload = owner.to_string();
    let outcome = tokio::time::timeout(
        READY_WAKE_DEADLINE,
        bus.publish(&Subject::ThreadWake(owner), &payload),
    )
    .await;
    let failure = match outcome {
        Ok(Ok(())) => return,
        Ok(Err(error)) => error,
        Err(elapsed) => elapsed.into(),
    };
    producers::fleet::ready_wake_failed(ReadyWake::Ring);
    let logged = failure.logged();
    let thread_id = payload.as_str();
    {
        let (event, error_code, reason, message) = (
            &"turn_wake_ring_failed",
            &logged.error_code,
            &logged.reason,
            format_args!("notification was lost; the next Postgres probe finds the admission"),
        );
        tracing::warn!(message, event, error_code, reason, thread_id);
    };
}
