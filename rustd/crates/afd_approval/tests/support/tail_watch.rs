//! What the inbox's live-tail suites all need: the two reads, and the values
//! the fixtures write.
//!
//! Split from `integration_inbox_tail.rs` when that file passed the 350-line
//! cap. The suites divide by CONCERN — announcement, continuation, runless —
//! and every one of them subscribes, waits for a frame, and answers as an
//! operator, so the waiting and the spellings live here rather than three
//! times over.

use std::time::Duration;

use afd_dragonfly::hub::Received;
use afd_dragonfly::{ReadyIndex, Subscription};
use serde_json::Value;

use crate::lane::Lane;

/// Who answers, when a test needs an operator.
pub(crate) const OPERATOR: &str = "human:fixture";

/// The note an operator leaves.
pub(crate) const NOTE: &str = "looks right";

/// The resolver a swept gate records, mirrored from the store.
pub(crate) const SWEEPER: &str = "system:approval_gate_sweeper";

/// The actor prefix a continuation is recorded under.
///
/// Mirrored from `afd_approval::inbox`, which keeps it private. A fixture
/// standing in for the lease path must write the admission BYTE-identically
/// or the payload digest diverges and the ledger mints a second id — which
/// would prove the opposite of what the converged-continuation proof asserts.
pub(crate) const CONTINUATION_ACTOR_PREFIX: &str = "continuation:";

/// The body a continuation carries, mirrored from the same module.
pub(crate) const CONTINUATION_BODY: &str = "{}";

/// How long a published frame is given to reach the subscriber.
pub(crate) const FRAME_DEADLINE: Duration = Duration::from_secs(5);

/// How long the hub's pump is given to register the subscription with Dragonfly.
pub(crate) const SUBSCRIBE_SETTLE: Duration = Duration::from_millis(250);

/// The next frame on the tail, as JSON, or `None` if none arrives in time.
pub(crate) async fn next_frame(tail: &mut Subscription) -> Option<Value> {
    let received = tokio::time::timeout(FRAME_DEADLINE, tail.recv())
        .await
        .ok()?;
    let Received::Message(message) = received.expect("the subscription stays live") else {
        return None;
    };
    serde_json::from_str(&message.payload).ok()
}

/// The current ready token for the lane's fleet.
pub(crate) async fn ready_token(lane: &Lane) -> Option<String> {
    ReadyIndex::new(lane.queue.clone())
        .token_for(lane.fleet.as_str())
        .await
        .expect("the ready index is readable")
        .map(|token| token.as_str().to_owned())
}
