//! The frames that go before the tick: a lease's first frame and its first
//! answer, the accounting they leave untouched, and the clock the run's
//! frames are read against.
#![expect(
    clippy::unwrap_used,
    reason = "test target: a fixture that cannot be built is a broken test"
)]

use std::time::Duration;

use afd_core::id::Uuid7;
use afd_wire::activity::ActivityAccepted;
use afr_agent::EventSink as _;
use afr_telemetry::labels::FrameDrop;
use afr_telemetry::testing::{Recorded, Tally, scoped};

use super::tests::{CALL, TOOL, chunk, completed, posted_frames, progress, started, tool};
use super::{MAX_BATCH_BYTES, channel};
use crate::test_support::{Answer, LEASE_ID, drain, json, plane};

/// A second tool call, open while the answer streams.
const SECOND_CALL: &str = "call-2";
/// The answer text every chunk here carries.
const ANSWER: &str = "hello";
/// The model time between the second call's start and the first chunk.
const MODEL_STEP: Duration = Duration::from_millis(60);
/// How long the first tool call ran.
const FIRST_CALL: Duration = Duration::from_millis(40);
/// What a completion frame says its call took; a paired call is timed by its
/// frames instead, so the figure is not read.
const OWN_MS: i64 = 60;

/// The lease's first frame is handed to the poster the moment it arrives,
/// once; the frame after it waits for the tick as every later one does.
#[tokio::test(start_paused = true)]
async fn test_first_frame_posts_without_waiting() {
    let (plane, mut calls) = plane(|_call| json(&ActivityAccepted { ok: true }));
    let lease = Uuid7::parse(LEASE_ID).unwrap();
    let (sink, mut pump) = channel(&plane, &lease);

    let ((), ()) = tokio::join!(pump.run(), async {
        sink.emit(tool(TOOL));
        tokio::time::sleep(Duration::from_millis(1)).await;
        assert_eq!(drain(&mut calls).len(), 1, "the first frame went at once");
        sink.emit(tool(TOOL));
        tokio::time::sleep(super::FLUSH_EVERY / 2).await;
        assert_eq!(drain(&mut calls).len(), 0, "the second waits for the tick");
        tokio::time::sleep(super::FLUSH_EVERY).await;
        assert_eq!(drain(&mut calls).len(), 1, "and goes on it");
        drop(sink);
    });
    assert_eq!(pump.dropped, 0);
}

/// The first answer chunk flushes too, carrying the frames buffered before
/// it ahead of it, so the screen shows the call and then the answer in the
/// order the run produced them and without waiting for the tick.
#[tokio::test(start_paused = true)]
async fn test_first_answer_chunk_flushes_in_order() {
    let (plane, mut calls) = plane(|_call| json(&ActivityAccepted { ok: true }));
    let lease = Uuid7::parse(LEASE_ID).unwrap();
    let (sink, mut pump) = channel(&plane, &lease);

    let ((), ()) = tokio::join!(pump.run(), async {
        sink.emit(started(CALL));
        sink.emit(progress(CALL));
        sink.emit(completed(CALL, 3));
        sink.emit(chunk(ANSWER.to_owned()));
        tokio::time::sleep(Duration::from_millis(1)).await;
        let posted = posted_frames(&drain(&mut calls));
        assert_eq!(
            posted,
            vec![
                vec!["tool_call_started".to_owned()],
                vec![
                    "tool_call_progress".to_owned(),
                    "tool_call_completed".to_owned(),
                    "fleet_response_chunk".to_owned()
                ],
            ],
            "the first frame alone, then the buffered frames ahead of the first answer"
        );
        drop(sink);
    });
    assert_eq!(pump.dropped, 0);
}

/// The early flush changes when a batch is handed over, never what is
/// counted: with the poster stalled and every slot held, exactly the batches
/// that filled while they waited are dropped, and the frames the pump still
/// holds when it is abandoned are counted as such.
#[tokio::test(start_paused = true)]
async fn test_early_flush_keeps_drop_accounting() {
    let (plane, _calls) = plane(|_call| Answer::Stall);
    let lease = Uuid7::parse(LEASE_ID).unwrap();
    let (sink, mut pump) = channel(&plane, &lease);
    // The same burst as `test_activity_sender_drops_past_four_batches`: the
    // first frame goes at once instead of on the second frame's arrival, the
    // next two fill the queue, the fourth fills while it waits and is dropped
    // when the fifth arrives, and the fifth, the run's last, waits for room.
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

    assert!(stalled.is_err(), "the daemon never answers");
    assert_eq!(
        pump.dropped, 1,
        "one full batch dropped, as without the flush"
    );
    assert_eq!(
        recorded.try_iter().collect::<Vec<_>>(),
        vec![Recorded::FramesDropped(FrameDrop::Backpressure, 1)]
    );
    let (tally, recorded) = Tally::new();
    scoped(tally, async { drop(pump) }).await;
    assert_eq!(
        recorded.try_iter().collect::<Vec<_>>(),
        vec![Recorded::FramesDropped(FrameDrop::Abandoned, 4)],
        "one in flight, two queued and the last waiting are abandoned"
    );
}

/// The run's clock starts when the run does: a chunk emitted before the run
/// starts is timed from the sink's making, one after from the start, and a
/// tool call's frames bracket the run's tool time, a cut call to the end.
#[tokio::test(start_paused = true)]
async fn test_the_run_clock_splits_tools_from_the_model() {
    let (plane, _calls) = plane(|_call| json(&ActivityAccepted { ok: true }));
    let lease = Uuid7::parse(LEASE_ID).unwrap();
    let (sink, _pump) = channel(&plane, &lease);
    tokio::time::advance(Duration::from_millis(500)).await;
    sink.run_started();
    sink.run_started();
    tokio::time::advance(Duration::from_millis(100)).await;
    sink.emit(started(CALL));
    tokio::time::advance(FIRST_CALL).await;
    sink.emit(completed(CALL, 40));
    sink.emit(started(SECOND_CALL));
    tokio::time::advance(MODEL_STEP).await;
    sink.emit(chunk(ANSWER.to_owned()));
    sink.emit(progress(SECOND_CALL));
    sink.emit(completed("call-3", -1));
    tokio::time::advance(FIRST_CALL).await;

    let timing = sink.timing();
    assert_eq!(
        timing.elapsed,
        Duration::from_millis(240),
        "from the start, not the making"
    );
    assert_eq!(timing.first_chunk, Some(Duration::from_millis(200)));
    assert_eq!(
        timing.tools,
        Duration::from_millis(140),
        "the first call's 40 ms, and the cut call's 100 ms to now"
    );
    assert_eq!(timing.model(), Duration::from_millis(100));
}

/// Calls that overlap, as a child's and the root's do, count once: the run's
/// tool time is the time any call was open, so tools and model never add up
/// to more than the run.
#[tokio::test(start_paused = true)]
async fn test_overlapping_calls_count_their_union() {
    let (plane, _calls) = plane(|_call| json(&ActivityAccepted { ok: true }));
    let lease = Uuid7::parse(LEASE_ID).unwrap();
    let (sink, _pump) = channel(&plane, &lease);
    sink.run_started();
    sink.emit(started(CALL));
    tokio::time::advance(MODEL_STEP).await;
    sink.emit(started(SECOND_CALL));
    tokio::time::advance(MODEL_STEP).await;
    sink.emit(completed(CALL, 120));
    tokio::time::advance(MODEL_STEP).await;
    sink.emit(completed(SECOND_CALL, 120));
    sink.emit(completed("unpaired", 30));
    tokio::time::advance(FIRST_CALL).await;

    let timing = sink.timing();
    assert_eq!(
        timing.tools,
        Duration::from_millis(210),
        "180 open, then 30 unpaired"
    );
    assert_eq!(timing.elapsed, Duration::from_millis(220));
    assert_eq!(timing.model(), Duration::from_millis(10));
}

/// A child reusing a call id keeps that id open until its last completion,
/// so the stretch it covers is not closed by the first one.
#[tokio::test(start_paused = true)]
async fn test_a_reused_call_id_stays_open_until_its_last_completion() {
    let (plane, _calls) = plane(|_call| json(&ActivityAccepted { ok: true }));
    let lease = Uuid7::parse(LEASE_ID).unwrap();
    let (sink, _pump) = channel(&plane, &lease);
    sink.run_started();
    sink.emit(started(CALL));
    sink.emit(started(CALL));
    tokio::time::advance(MODEL_STEP).await;
    sink.emit(completed(CALL, OWN_MS));
    tokio::time::advance(MODEL_STEP).await;
    sink.emit(completed(CALL, OWN_MS));
    tokio::time::advance(FIRST_CALL).await;

    let timing = sink.timing();
    assert_eq!(
        timing.tools,
        MODEL_STEP * 2,
        "open until the second completion"
    );
    assert_eq!(timing.model(), FIRST_CALL);
}
