//! Native tool activity retains its lease-scoped call identity and bounded outcome.
#![cfg(feature = "test-util")]
#![expect(clippy::expect_used, reason = "integration preconditions fail loudly")]

use af_wire::activity::CALL_ID_MAX_BYTES;
use af_wire::tool_trace::OUTPUT_EDGE_MAX_BYTES;
use agentsfleetd::supervisor::Supervisor;
use serde_json::json;

use crate::integration_runner_activity::{TOOL_NAME, Tailed};
use crate::wire::field;

const CALL_ID: &str = "3";

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_activity_carries_a_required_call_id() {
    let mut supervisor = Supervisor::new();
    let mut tailed = Tailed::open(&mut supervisor).await;
    tailed
        .forward(&json!({"frames": [{"tool_call_completed": {
            "name":TOOL_NAME, "ms":5, "call_id":CALL_ID, "status":"succeeded"
        }}]}))
        .await;
    let frame = tailed
        .next_frame()
        .await
        .expect("the named completion publishes");
    assert_eq!(
        field(&frame, "call_id"),
        &json!(format!("{}:{CALL_ID}", tailed.fence)),
        "the runner call identity is scoped to its lease fence"
    );
    for refused in [
        json!({"frames": [{"tool_call_progress": {"name":TOOL_NAME, "elapsed_ms":5}}]}),
        json!({"frames": [{"tool_call_started": {"name":TOOL_NAME, "args_redacted":"{}", "call_id":""}}]}),
        json!({"frames": [{"tool_call_started": {"name":TOOL_NAME, "args_redacted":"{}", "call_id":"c".repeat(CALL_ID_MAX_BYTES + 1)}}]}),
        json!({"frames": [{"tool_call_completed": {"name":TOOL_NAME, "ms":5, "call_id":CALL_ID, "status":"succeeded", "retries":1}}]}),
    ] {
        tailed = tailed.refused(&refused).await;
    }
    tailed
        .forward(&json!({"frames": [{"tool_call_started": {
            "name":TOOL_NAME, "args_redacted":"{\"credential\":\"[redacted]\"}", "call_id":CALL_ID
        }}]}))
        .await;
    let frame = tailed
        .next_frame()
        .await
        .expect("redacted arguments publish as JSON");
    assert_eq!(
        field(&frame, "args_redacted"),
        &json!({"credential":"[redacted]"})
    );
    assert_eq!(
        field(&frame, "call_id"),
        &json!(format!("{}:{CALL_ID}", tailed.fence))
    );
    tailed.finish(supervisor).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_activity_tool_outcome_reaches_channel() {
    let mut supervisor = Supervisor::new();
    let mut tailed = Tailed::open(&mut supervisor).await;
    tailed
        .forward(&json!({"frames": [{"tool_call_completed": {
            "name":TOOL_NAME, "ms":5, "call_id":CALL_ID, "status":"failed",
            "output_head":"error: denied", "output_tail":"exit 2",
            "output_line_count":40, "exit_code":2
        }}]}))
        .await;
    let frame = tailed.next_frame().await.expect("the completion publishes");
    assert_eq!(field(&frame, "kind"), &json!("tool_call_completed"));
    assert_eq!(field(&frame, "status"), &json!("failed"));
    assert_eq!(field(&frame, "output_head"), &json!("error: denied"));
    assert_eq!(field(&frame, "output_tail"), &json!("exit 2"));
    assert_eq!(field(&frame, "output_line_count"), &json!(40));
    assert_eq!(field(&frame, "exit_code"), &json!(2));
    // A valid prefix must not publish when a later frame violates the batch's bounds.
    tailed = tailed
        .refused(&json!({"frames": [
            {"fleet_response_chunk":{"text":"invalid batch prefix", "text_kind":"answer"}},
            {"tool_call_completed": {
                "name":TOOL_NAME, "ms":5, "call_id":CALL_ID, "status":"failed",
                "output_tail":"a".repeat(OUTPUT_EDGE_MAX_BYTES + 1)
            }}
        ]}))
        .await;
    assert_eq!(
        tailed.silence().await,
        None,
        "no oversized edge or valid prefix published"
    );
    tailed.finish(supervisor).await;
}
