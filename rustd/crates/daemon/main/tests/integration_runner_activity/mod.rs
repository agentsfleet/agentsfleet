//! Native activity publishes through the Bus only after decoding and lease ownership checks.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "integration preconditions fail loudly"
)]

mod fixture;
pub(crate) use fixture::Tailed;

use agentsfleetd::supervisor::Supervisor;
use serde_json::json;

use crate::wire::field;

pub(crate) const TOOL_NAME: &str = "shell";
const CHUNK_TEXT: &str = "the fleet said this out loud";
const UNHELD_LEASE: &str = "0195b4ba-8d3a-7fff-8abc-ffffffffffff";

/// A valid batch publishes; malformed and unowned activity have no effect.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_activity_publish() {
    let mut supervisor = Supervisor::new();
    let mut tailed = Tailed::open(&mut supervisor).await;
    tailed
        .forward(&json!({"frames": [{"fleet_response_chunk": {
            "text": CHUNK_TEXT, "text_kind": "answer"
        }}]}))
        .await;
    let frame = tailed
        .next_frame()
        .await
        .expect("the frame reaches the tail");
    assert_eq!(field(&frame, "kind"), &json!("chunk"));
    assert_eq!(field(&frame, "text"), &json!(CHUNK_TEXT));
    assert_eq!(
        field(&frame, "event_id"),
        &json!(tailed.run.event_id),
        "activity is grouped by the event, never the lease"
    );
    tailed = tailed
        .refused(&json!({"frames": [{"no_such_frame": {"text": "?"}}]}))
        .await;

    tailed.finish(supervisor).await;
}

/// A rendering failure drops that frame while a later valid frame still publishes.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_activity_drops_a_frame_it_cannot_render() {
    let mut supervisor = Supervisor::new();
    let mut tailed = Tailed::open(&mut supervisor).await;
    tailed
        .forward(&json!({"frames": [{"tool_call_started": {
            "name":TOOL_NAME, "args_redacted":"this is not JSON", "call_id":"1"
        }}]}))
        .await;
    assert_eq!(
        tailed.silence().await,
        None,
        "unrenderable arguments do not publish"
    );
    tailed
        .forward(&json!({"frames": [{"fleet_response_chunk": {
            "text":CHUNK_TEXT, "text_kind":"answer"
        }}]}))
        .await;
    let frame = tailed
        .next_frame()
        .await
        .expect("one dropped frame does not poison the lease");
    assert_eq!(field(&frame, "text"), &json!(CHUNK_TEXT));
    assert_eq!(
        tailed.silence().await,
        None,
        "only the renderable frame publishes"
    );
    tailed.finish(supervisor).await;
}

/// An unowned frame cannot publish, and the same connection still serves its real lease.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn an_unowned_native_activity_frame_reaches_no_tail() {
    let mut supervisor = Supervisor::new();
    let mut tailed = Tailed::open(&mut supervisor).await;
    tailed
        .forward_for(
            UNHELD_LEASE,
            &json!({"frames": [{"fleet_response_chunk": {
                "text":"an unowned lease must not publish", "text_kind":"answer"
            }}]}),
        )
        .await;
    assert_eq!(
        tailed.silence().await,
        None,
        "an unowned lease has no cached target"
    );
    tailed
        .forward(&json!({"frames": [{"fleet_response_chunk": {
            "text":CHUNK_TEXT, "text_kind":"answer"
        }}]}))
        .await;
    let frame = tailed
        .next_frame()
        .await
        .expect("the native session remains usable");
    assert_eq!(field(&frame, "text"), &json!(CHUNK_TEXT));
    assert_eq!(
        tailed.silence().await,
        None,
        "no unowned frame arrived late"
    );
    tailed.finish(supervisor).await;
}
