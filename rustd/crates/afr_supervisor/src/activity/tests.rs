#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test target: a fixture that cannot be built is a broken test"
)]

use std::io::Write as _;
use std::time::Duration;

use afd_core::id::Uuid7;
use afd_wire::activity::{ActivityAccepted, ActivityFrame, FleetResponseChunk, ToolCallStarted};
use afr_agent::EventSink as _;

use super::{Counter, MAX_BATCH_BYTES, channel, encoded_len};
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
    assert_eq!(pump.dropped, 1, "four held, the fifth dropped");
}

#[tokio::test(start_paused = true)]
async fn frames_ride_together_and_the_first_answer_chunk_is_timed() {
    let (plane, mut calls) = plane(|_call| json(&ActivityAccepted { ok: true }));
    let lease = Uuid7::parse(LEASE_ID).unwrap();
    let (sink, mut pump) = channel(&plane, &lease);
    sink.emit(tool(TOOL));
    assert_eq!(sink.first_chunk(), None, "a tool frame is not an answer");
    tokio::time::advance(Duration::from_millis(120)).await;
    sink.emit(chunk("hello".to_owned()));
    tokio::time::advance(Duration::from_millis(80)).await;
    sink.emit(chunk("again".to_owned()));
    let first = sink.first_chunk();
    drop(sink);

    pump.run().await;

    assert_eq!(
        first,
        Some(Duration::from_millis(120)),
        "the first chunk, not the latest"
    );
    let posted = drain(&mut calls);
    assert_eq!(posted.len(), 1);
    assert_eq!(posted[0].verb, Verb::Activity);
    let body: serde_json::Value = serde_json::from_slice(posted[0].body.as_ref().unwrap()).unwrap();
    assert_eq!(body["frames"].as_array().unwrap().len(), 3);
    assert_eq!(pump.dropped, 0);
}

#[tokio::test(start_paused = true)]
async fn a_frame_after_a_quiet_spell_waits_a_full_period_before_it_is_sent() {
    let (plane, mut calls) = plane(|_call| json(&ActivityAccepted { ok: true }));
    let lease = Uuid7::parse(LEASE_ID).unwrap();
    let (sink, mut pump) = channel(&plane, &lease);

    let ((), ()) = tokio::join!(pump.run(), async {
        tokio::time::sleep(Duration::from_secs(10)).await;
        sink.emit(tool(TOOL));
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(
            drain(&mut calls).len(),
            0,
            "a stale tick does not fire at once"
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(
            drain(&mut calls).len(),
            1,
            "sent one period after it arrived"
        );
        drop(sink);
    });
}

#[tokio::test(start_paused = true)]
async fn a_failed_post_is_only_logged() {
    let (plane, mut calls) = plane(|_call| Answer::Fail(error::unavailable(Verb::Activity, 503)));
    let lease = Uuid7::parse(LEASE_ID).unwrap();
    let (sink, mut pump) = channel(&plane, &lease);
    sink.emit(tool(TOOL));
    drop(sink);

    pump.run().await;

    assert_eq!(drain(&mut calls).len(), 1, "posted once, not retried");
}

#[test]
fn a_frame_is_measured_as_encoded_and_the_counter_keeps_count() {
    let frame = tool(TOOL);
    let mut counter = Counter(0);
    counter.write_all(b"abc").unwrap();
    counter.flush().unwrap();

    assert_eq!(
        encoded_len(&frame),
        serde_json::to_vec(&frame).unwrap().len()
    );
    assert_eq!(counter.0, 3, "a flush keeps the count");
}
