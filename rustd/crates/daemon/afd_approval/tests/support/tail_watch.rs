//! What the inbox's live-tail suites all need: the subscription, the two reads,
//! and the values the fixtures write.
//!
//! Split from `integration_inbox_tail.rs` when that file passed the 350-line
//! cap. The suites divide by CONCERN — the announcement and the answer turn —
//! and both subscribe, wait for a frame, and answer as an operator, so the
//! waiting and the spellings live here rather than twice over.

use std::borrow::Cow;
use std::time::Duration;

use af_core::id::Uuid7;
use afd_bus::{Received, Subject, Subscription};
use serde_json::Value;

use crate::lane::Lane;

/// Who answers, when a test needs an operator.
pub(crate) const OPERATOR: &str = "human:fixture";

/// The note an operator leaves.
pub(crate) const NOTE: &str = "looks right";

/// The actor prefix an answer turn is recorded under.
///
/// Mirrored from `afd_approval::inbox`, which keeps it private. A fixture
/// standing in for the run path writes the turn as the resolve would, so the
/// converged-answer proof compares like with like.
pub(crate) const CONTINUATION_ACTOR_PREFIX: &str = "continuation:";

/// The body an answer turn carries: what the person decided, and the turn
/// whose run waits on it, as a question's answer is written.
pub(crate) fn answer_body(message: &str, asked: &Uuid7) -> Value {
    serde_json::json!(af_wire::turn::QuestionAnswer {
        message: Cow::Borrowed(message),
        asked_turn_id: Cow::Borrowed(asked.as_str()),
    })
}

/// How long a published frame is given to reach the subscriber.
pub(crate) const FRAME_DEADLINE: Duration = Duration::from_secs(5);

/// The lane's fleet's live tail. The bus registers the reader before this
/// returns, so every frame published after it reaches the tail.
pub(crate) async fn tail(lane: &Lane) -> Subscription {
    lane.bus
        .subscribe(&Subject::ThreadItems(lane.fleet.clone()))
        .await
        .expect("the lane's bus takes a subscriber")
}

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
