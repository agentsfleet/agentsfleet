#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test target: a fixture that cannot be built is a broken test"
)]

use std::time::Duration;

use afd_core::id::Uuid7;
use afd_wire::activity::{ActivityFrame, FleetResponseChunk, ToolCallStarted};
use afr_agent::EventSink as _;

use super::{MAX_BATCH_BYTES, channel, encoded_len};
use crate::client::Verb;
use crate::error;
use crate::test_support::{Answer, LEASE_ID, drain, json, plane};

/// The tool every frame here names.
const TOOL: &str = "shell";

fn chunk(text: String) -> ActivityFrame<'static> {
    ActivityFrame::FleetResponseChunk(FleetResponseChunk {
        text: text.into(),
        text_kind: None,
        first_chunk_after_ms: None,
        stream_start: false,
        stream_contiguous: true,
        stream_seq: 1,
    })
}

fn tool(name: &'static str) -> ActivityFrame<'static> {
    ActivityFrame::ToolCallStarted(ToolCallStarted {
        name: name.into(),
        args_redacted: "{}".into(),
        call_id: None,
    })
}

#[tokio::test(start_paused = true)]
async fn test_activity_sender_drops_past_four_batches() {
    let (plane, _calls) = plane(|_call| Answer::Stall);
    let lease = Uuid7::parse(LEASE_ID).unwrap();
    let (sink, mut pump) = channel(&plane, &lease);
    // Each frame fills more than half a batch, so each rides alone.
    for _ in 0..5 {
        sink.emit(chunk("x".repeat(MAX_BATCH_BYTES / 2 + 1)));
    }
    drop(sink);

    let stalled = tokio::time::timeout(Duration::from_secs(5), pump.run()).await;

    assert!(
        stalled.is_err(),
        "the daemon never answers, so the pump never drains"
    );
    assert_eq!(pump.pumped().dropped, 1, "four held, the fifth dropped");
}

#[tokio::test(start_paused = true)]
async fn frames_ride_together_and_post_once_the_run_ends() {
    let (plane, mut calls) = plane(|_call| json(&serde_json::json!({"ok": true})));
    let lease = Uuid7::parse(LEASE_ID).unwrap();
    let (sink, mut pump) = channel(&plane, &lease);
    sink.emit(tool(TOOL));
    sink.emit(chunk("hello".to_owned()));
    drop(sink);

    pump.run().await;

    let posted = drain(&mut calls);
    assert_eq!(posted.len(), 1);
    assert_eq!(posted[0].verb, Verb::Activity);
    let body: serde_json::Value = serde_json::from_slice(posted[0].body.as_ref().unwrap()).unwrap();
    assert_eq!(body["frames"].as_array().unwrap().len(), 2);
    assert_eq!(pump.pumped().dropped, 0);
    assert!(
        pump.pumped().first_chunk.is_some(),
        "the answer chunk marks the first token"
    );
}

#[tokio::test(start_paused = true)]
async fn a_partial_batch_is_sent_on_the_flush_tick_and_a_failed_post_is_only_logged() {
    let (plane, mut calls) = plane(|_call| Answer::Fail(error::unavailable(Verb::Activity, 503)));
    let lease = Uuid7::parse(LEASE_ID).unwrap();
    let (sink, mut pump) = channel(&plane, &lease);
    sink.emit(tool(TOOL));

    let still_running = tokio::time::timeout(Duration::from_secs(1), pump.run()).await;
    drop(sink);
    pump.run().await;

    assert!(still_running.is_err());
    assert_eq!(
        drain(&mut calls).len(),
        1,
        "flushed by the tick while the run went on"
    );
    assert_eq!(
        pump.pumped().first_chunk,
        None,
        "a tool frame is not an answer"
    );
}

#[test]
fn a_frame_is_measured_as_encoded() {
    let frame = tool(TOOL);
    let mut counter = super::Counter(0);
    std::io::Write::write_all(&mut counter, b"abc").unwrap();
    std::io::Write::flush(&mut counter).unwrap();

    assert_eq!(
        encoded_len(&frame),
        serde_json::to_vec(&frame).unwrap().len()
    );
    assert_eq!(counter.counted(), 3, "a flush keeps the count");
}

#[test]
fn emitting_after_the_pump_is_gone_is_harmless() {
    let (plane, _calls) = plane(|_call| Answer::Stall);
    let lease = Uuid7::parse(LEASE_ID).unwrap();
    let (sink, pump) = channel(&plane, &lease);
    drop(pump);

    sink.emit(tool(TOOL));
}
