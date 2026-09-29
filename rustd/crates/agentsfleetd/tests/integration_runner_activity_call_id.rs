//! A runner tool frame's optional call id, on its way to the live tail.
//!
//! Split from `integration_runner_activity.rs` at the length cap, and shares
//! its [`Tailed`] fixture: the property is a publish, so it needs a subscriber
//! running while the request is in flight.
//!
//! Marked `#[ignore]` like the rest of the live-service suite; run by
//! `make test-integration-rustd`.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly, and a missing lane knob is one"
)]

use afd_wire::activity::CALL_ID_MAX_BYTES;
use agentsfleetd::supervisor::Supervisor;
use serde_json::json;

use crate::integration_runner_activity::{TOOL_NAME, Tailed, code_of};
use crate::tail::{next_frame, silence};
use crate::wire::field;

/// The call id every frame of one tool call carries.
const CALL_ID: &str = "3";

/// A tool frame's `call_id` is optional, bounded, and republished scoped to its
/// lease as `{fence}:{call_id}`; a frame without one publishes as it always
/// did; any other unknown field still refuses the batch.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_activity_carries_an_optional_call_id() {
    let mut supervisor = Supervisor::new();
    let mut tailed = Tailed::open(&mut supervisor).await;

    let named = json!({"frames": [{"tool_call_completed": {"name": TOOL_NAME, "ms": 5, "call_id": CALL_ID}}]});
    assert_eq!(tailed.forward(&named).await.status().as_u16(), 202);
    let frame = next_frame(&mut tailed.tail)
        .await
        .expect("the named frame publishes");
    assert_eq!(
        field(&frame, "call_id"),
        &json!(format!("{}:{CALL_ID}", tailed.fence)),
        "the runner's id, scoped to the lease that sent it"
    );

    let unnamed = json!({"frames": [{"tool_call_progress": {"name": TOOL_NAME, "elapsed_ms": 5}}]});
    assert_eq!(tailed.forward(&unnamed).await.status().as_u16(), 202);
    let frame = next_frame(&mut tailed.tail)
        .await
        .expect("the unnamed frame publishes");
    assert!(
        frame.get("call_id").is_none(),
        "no call id is invented: {frame}"
    );

    for refused in [
        json!({"frames": [{"tool_call_started": {"name": TOOL_NAME, "args_redacted": "{}", "call_id": ""}}]}),
        json!({"frames": [{"tool_call_started": {"name": TOOL_NAME, "args_redacted": "{}", "call_id": "c".repeat(CALL_ID_MAX_BYTES + 1)}}]}),
        json!({"frames": [{"tool_call_completed": {"name": TOOL_NAME, "ms": 5, "call_id": CALL_ID, "retries": 1}}]}),
    ] {
        let answer = tailed.forward(&refused).await;
        assert_eq!(answer.status().as_u16(), 400, "{refused}");
        assert_eq!(
            code_of(answer).await,
            afd_core::error_code::INVALID_REQUEST.as_str()
        );
    }
    assert_eq!(
        silence(&mut tailed.tail).await,
        None,
        "no refused batch published"
    );

    drop(tailed.tail);
    supervisor.shutdown().await;
    tailed.run.cleanup().await;
}
