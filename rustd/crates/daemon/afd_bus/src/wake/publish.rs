use af_core::{error_code::Coded as _, id::Uuid7, timing::READY_WAKE_DEADLINE};
use af_observability::{metrics::label::fleet::ReadyWake, producers};

use crate::{Handle, Subject};

/// Publish only after commit; failure cannot reject work already accepted.
pub async fn publish(bus: &Handle, fleet: Uuid7) {
    let payload = fleet.to_string();
    let shard = crc16::State::<crc16::XMODEM>::calculate(payload.as_bytes()) % super::SHARDS;
    let outcome = tokio::time::timeout(
        READY_WAKE_DEADLINE,
        bus.publish(&Subject::Wake(shard), &payload),
    )
    .await;
    let failure = match outcome {
        Ok(Ok(())) => return,
        Ok(Err(error)) => error,
        Err(elapsed) => elapsed.into(),
    };
    producers::fleet::ready_wake_failed(ReadyWake::Ring);
    let logged = failure.logged();
    let fleet_id = payload.as_str();
    {
        let (event, error_code, reason, message) = (
            &"admission_wake_ring_failed",
            &logged.error_code,
            &logged.reason,
            format_args!("notification was lost; the next Postgres probe finds the admission"),
        );
        tracing::warn!(message, event, error_code, reason, fleet_id, shard);
    };
}
