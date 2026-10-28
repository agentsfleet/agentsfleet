//! Prove transport progress and private publication against live Postgres and Dragonfly.
#![expect(
    clippy::print_stderr,
    reason = "the test prints its measured evidence for the lane log"
)]

mod compaction;
mod deadline;
mod fixture;
mod privacy;

use crate::draft::{attempt::Attempt, fixture::Fixture};
use afd_authoring::{ModelCallInput, ModelCallResult, types::Source};
use fixture::{Live, WAIT};
use std::{sync::atomic::Ordering, time::Instant};

const MILLIS_PER_SECOND: f64 = 1_000.0;

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires live Postgres and Dragonfly: make test-integration-rustd"]
async fn draft_stream_delivers_before_completion_and_final_replay_never_resends() {
    let f = Fixture::create().await;
    let attempt = Attempt::start(&f, f.access(), Source::Blank).await;
    let mut live = Live::start(&f, &attempt).await;
    let request = super::fixture::request(&attempt);
    let task = live.call(&f.runner, &attempt, request.clone());
    live.arrived().await;
    let began = Instant::now();
    live.text("first answer").await;
    let first = live.next().await;
    let first_ms = began.elapsed().as_secs_f64() * MILLIS_PER_SECOND;
    assert_eq!(first.text, "first answer");
    assert_eq!(first.draft_id, attempt.draft.id);
    assert_eq!(first.sequence, "1");
    assert_eq!(first.stream_id.as_str(), request.dispatch_id);
    assert_eq!(first.stream_seq, 1);
    assert!(first.stream_start);
    assert!(
        !task.is_finished(),
        "incremental delivery must precede provider completion"
    );
    live.frame(serde_json::json!({"type":"response.reasoning_summary_text.delta","delta":"private reasoning"})).await;
    live.text("second answer").await;
    let second = live.next().await;
    assert_eq!(second.text, "second answer");
    assert_eq!(second.stream_seq, 2);
    assert!(!second.stream_start);
    live.finish().await;
    let result = tokio::time::timeout(WAIT, task)
        .await
        .expect("receipt completion")
        .expect("dispatch task")
        .expect("known final");
    replay(&live, &f, &attempt, request, result).await;
    assert_eq!(live.provider.calls.load(Ordering::SeqCst), 1);
    assert_eq!(super::fixture::receipts(&f, &attempt).await, (1, 1, false));
    live.drained().await;
    let channel = afd_authoring::draft_channel(&attempt.draft.id);
    assert_eq!(live.hub.readers(&channel), 1);
    drop(live.reader);
    assert_eq!(live.hub.readers(&channel), 0);
    eprintln!("private_stream first_text_ms={first_ms:.3} physical_sends=1 final_receipts=1");
    f.cleanup().await;
}

async fn replay(
    live: &Live,
    f: &Fixture,
    attempt: &Attempt,
    request: ModelCallInput,
    mut result: ModelCallResult,
) {
    let replay = live
        .calls
        .call(&f.runner, &attempt.lease, request)
        .await
        .expect("durable final replay");
    assert!(!result.replayed);
    assert!(replay.replayed);
    result.replayed = true;
    assert_eq!(
        serde_json::to_value(result).expect("result"),
        serde_json::to_value(replay).expect("replay")
    );
}
