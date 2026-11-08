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
    tracing::warn!(
        event = "admission_wake_ring_failed",
        error_code = logged.error_code,
        reason = logged.reason,
        fleet_id,
        shard,
        "notification was lost; the next Postgres probe finds the admission"
    );
}
