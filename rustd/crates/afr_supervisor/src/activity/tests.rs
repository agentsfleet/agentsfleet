#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test target: a fixture that cannot be built is a broken test"
)]

use std::io::Write as _;
use std::time::Duration;

use afd_core::id::Uuid7;
use afd_wire::activity::{
    ActivityAccepted, ActivityFrame, FleetResponseChunk, ToolCallCompleted, ToolCallProgress,
    ToolCallStarted,
};
use afr_agent::EventSink as _;
use afr_telemetry::labels::FrameDrop;
use afr_telemetry::testing::{Recorded, Tally, scoped};

use super::{Counter, MAX_BATCH_BYTES, channel, encoded_len};
use crate::client::Verb;
use crate::error;
use crate::test_support::{Answer, LEASE_ID, drain, json, plane};

/// The tool every frame here names.
pub(super) const TOOL: &str = "shell";
/// The call id a bracketed tool call's frames share.
pub(super) const CALL: &str = "call-1";
/// The frames one posted batch carried.
const FRAMES: &str = "frames";

pub(super) fn chunk(text: String) -> ActivityFrame<'static> {
    ActivityFrame::FleetResponseChunk(FleetResponseChunk {
        text: text.into(),
        text_kind: None,
        first_chunk_after_ms: None,
        stream_start: false,
        stream_contiguous: true,
        stream_seq: 1,
    })
}

/// The redacted arguments of a call that took none.
const NO_ARGS: &str = "{}";
/// Long enough that only a stalled pump reaches it.
const STALL_BUDGET: Duration = Duration::from_secs(60);

pub(super) fn tool(name: &'static str) -> ActivityFrame<'static> {
    ActivityFrame::ToolCallStarted(ToolCallStarted {
        name: name.into(),
        args_redacted: NO_ARGS.into(),
        call_id: None,
    })
}

pub(super) fn started(call_id: &'static str) -> ActivityFrame<'static> {
    ActivityFrame::ToolCallStarted(ToolCallStarted {
        name: TOOL.into(),
        args_redacted: NO_ARGS.into(),
        call_id: Some(call_id.into()),
    })
}

pub(super) fn progress(call_id: &'static str) -> ActivityFrame<'static> {
    ActivityFrame::ToolCallProgress(ToolCallProgress {
        name: TOOL.into(),
        elapsed_ms: 1,
        call_id: Some(call_id.into()),
    })
}

pub(super) fn completed(call_id: &'static str, ms: i64) -> ActivityFrame<'static> {
    ActivityFrame::ToolCallCompleted(ToolCallCompleted {
        name: TOOL.into(),
        ms,
        call_id: Some(call_id.into()),
        status: None,
        output_head: None,
        output_tail: None,
        output_line_count: None,
        exit_code: None,
    })
}

/// The frames each activity post carried, as their wire variant names, in
/// the order they were posted.
pub(super) fn posted_frames(calls: &[crate::client::Call]) -> Vec<Vec<String>> {
    calls
        .iter()
        .filter(|call| call.verb == Verb::Activity)
        .map(|call| {
            let body: serde_json::Value =
                serde_json::from_slice(call.body.as_ref().unwrap()).unwrap();
            body[FRAMES]
                .as_array()
                .unwrap()
                .iter()
                .map(|frame| frame.as_object().unwrap().keys().next().unwrap().clone())
                .collect()
        })
        .collect()
}

#[tokio::test(start_paused = true)]
async fn test_activity_sender_drops_past_four_batches() {
    let (plane, _calls) = plane(|_call| Answer::Stall);
    let lease = Uuid7::parse(LEASE_ID).unwrap();
    let (sink, mut pump) = channel(&plane, &lease);
    // Each frame fills more than half a batch, so each rides alone. The burst
    // lands before the poster takes one in flight, so three queue; the fourth
    // fills while it waits and is dropped when the fifth arrives; the fifth,
    // the run's last, waits for room.
    for _ in 0..5 {
        sink.emit(chunk("x".repeat(MAX_BATCH_BYTES / 2 + 1)));
    }
    drop(sink);

    let (tally, recorded) = Tally::new();
    let stalled = scoped(
        tally,
        tokio::time::timeout(Duration::from_secs(5), pump.run()),
    )
    .await;

    assert!(
        stalled.is_err(),
        "the daemon never answers, so the pump never drains"
    );
    assert_eq!(
        pump.dropped, 1,
        "only the full batch that waited is dropped"
    );
    assert_eq!(
        recorded.try_iter().collect::<Vec<_>>(),
        vec![Recorded::FramesDropped(FrameDrop::Backpressure, 1)],
        "the dropped batch's one frame is counted"
    );
}

/// A stream of small frames behind four held batches joins one waiting batch
/// instead of losing a frame a tick.
#[tokio::test(start_paused = true)]
async fn test_activity_sender_keeps_filling_a_batch_that_waits() {
    let (plane, _calls) = plane(|_call| Answer::Stall);
    let lease = Uuid7::parse(LEASE_ID).unwrap();
    let (sink, mut pump) = channel(&plane, &lease);
    let tick = super::FLUSH_EVERY * 2;

    let (stalled, ()) = tokio::join!(tokio::time::timeout(STALL_BUDGET, pump.run()), async {
        // Each big frame is sent on its own tick and holds a slot.
        for _ in 0..4 {
            sink.emit(chunk("x".repeat(MAX_BATCH_BYTES / 2 + 1)));
            tokio::time::sleep(tick).await;
        }
        // Every slot is held now; these see tick after tick go by.
        for _ in 0..8 {
            sink.emit(tool(TOOL));
            tokio::time::sleep(tick).await;
        }
        drop(sink);
    });

    assert!(stalled.is_err(), "the daemon never answers");
    assert_eq!(
        pump.dropped, 0,
        "the small frames wait together, none dropped"
    );
}

/// After the lease's first frame and first answer have gone, frames ride
/// together on the tick, and the first answer chunk is timed from the run's
/// start rather than from whichever chunk came last.
#[tokio::test(start_paused = true)]
async fn frames_ride_together_and_the_first_answer_chunk_is_timed() {
    let (plane, mut calls) = plane(|_call| json(&ActivityAccepted { ok: true }));
    let lease = Uuid7::parse(LEASE_ID).unwrap();
    let (sink, mut pump) = channel(&plane, &lease);
    sink.emit(tool(TOOL));
    assert_eq!(
        sink.timing().first_chunk,
        None,
        "a tool frame is not an answer"
    );
    tokio::time::advance(Duration::from_millis(120)).await;
    sink.emit(chunk("hello".to_owned()));
    tokio::time::advance(Duration::from_millis(80)).await;
    sink.emit(chunk("again".to_owned()));
    sink.emit(tool(TOOL));
    let first = sink.timing().first_chunk;
    drop(sink);

    pump.run().await;

    assert_eq!(
        first,
        Some(Duration::from_millis(120)),
        "the first chunk, not the latest"
    );
    let posted = posted_frames(&drain(&mut calls));
    assert_eq!(
        posted.len(),
        3,
        "the first frame, the first answer, then the rest together: {posted:?}"
    );
    assert_eq!(posted[2].len(), 2, "the last two rode together");
    assert_eq!(pump.dropped, 0);
}

/// Once the lease's first frame and first answer have gone, a frame after a
/// quiet spell waits a full period rather than going on a stale tick.
#[tokio::test(start_paused = true)]
async fn a_frame_after_a_quiet_spell_waits_a_full_period_before_it_is_sent() {
    let (plane, mut calls) = plane(|_call| json(&ActivityAccepted { ok: true }));
    let lease = Uuid7::parse(LEASE_ID).unwrap();
    let (sink, mut pump) = channel(&plane, &lease);

    let ((), ()) = tokio::join!(pump.run(), async {
        sink.emit(chunk("first".to_owned()));
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert_eq!(drain(&mut calls).len(), 1, "the first frame went at once");
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

#[tokio::test(start_paused = true)]
async fn a_failed_post_is_only_logged() {
    let (plane, mut calls) = plane(|_call| Answer::Fail(error::unavailable(Verb::Activity, 503)));
    let lease = Uuid7::parse(LEASE_ID).unwrap();
    let (sink, mut pump) = channel(&plane, &lease);
    sink.emit(tool(TOOL));
    drop(sink);

    let (tally, recorded) = Tally::new();
    scoped(tally, pump.run()).await;

    assert_eq!(drain(&mut calls).len(), 1, "posted once, not retried");
    assert_eq!(
        recorded.try_iter().collect::<Vec<_>>(),
        vec![Recorded::FramesDropped(FrameDrop::PostFailed, 1)],
        "the frame the daemon did not take is counted as lost"
    );
}
