//! Timing an observer reads without prompts, response bodies or credentials.

use super::*;
use crate::wire::observe::{ObservedTokens, TurnObservation};
use crate::wire::recorder::Seen;

const DEADLINE: Duration = Duration::from_secs(1);

#[tokio::test]
async fn debug_rendering_excludes_private_request_and_response_content() {
    let wire = format!("{TEXT}{USAGE}{STOP}");
    let (turn, _) = open(
        Box::pin(stream::once(async move { Ok(Bytes::from(wire)) })),
        TurnControl::default(),
    );
    let pending = format!("{turn:?}");
    let result = format!("{:?}", turn.finish().await);
    assert!(pending.contains("StreamTurn"));
    assert!(result.contains("TurnResult"));
    for debug in [pending, result] {
        for private in ["private prompt", "private-credential", "Hi 🌿"] {
            assert!(!debug.contains(private), "private content reached Debug");
        }
    }
}

#[tokio::test]
async fn completed_stream_has_one_timing_pair_and_content_free_first_text() {
    let captured = Recorder::new();
    let wire = format!("{TEXT}{USAGE}{STOP}");
    let (turn, _) = open(
        Box::pin(stream::once(async move { Ok(Bytes::from(wire)) })),
        observing(&captured),
    );
    assert!(turn.finish().await.failure.is_none());
    let seen = captured.seen();
    assert_eq!(seen.first(), Some(&Seen::Started));
    assert_eq!(
        seen.iter().filter(|seen| **seen == Seen::Started).count(),
        1
    );
    assert_eq!(captured.first_texts().len(), 1);
    let ended = only(captured.ended());
    assert_eq!(ended.cause, None);
    assert_eq!(ended.text_bytes, 7);
    let calibration = only(captured.measured());
    assert_eq!(calibration.actual_input_tokens, 3);
    assert_eq!(calibration.method, "o200k_proxy_estimate");
    assert!(calibration.measured_input_tokens > 0);
    assert_ne!(calibration.revision, "");
    assert_eq!(calibration.route_id.len(), 64);
    // The observer publishes both digests, so they are pinned to the byte:
    // the SHA-256 of the route's JSON identity and of the model's name.
    // pin test: literal is the contract
    assert_eq!(
        calibration.route_id,
        "00f78eec7771164be3d0ce495945f454798e6d5d29a096905e556996d1d17f4c"
    );
    // pin test: literal is the contract
    assert_eq!(
        calibration.model_id,
        "4bbcf8b89b51d28551bd19b683dbe81726f2f4f1200b499ea8cbe05665f177f5"
    );
    let fields = format!("{seen:?}");
    for private in ["private prompt", "private-credential", "Hi 🌿"] {
        assert!(
            !fields.contains(private),
            "private provider content reached the observer"
        );
    }
}

#[tokio::test(start_paused = true)]
async fn deadline_has_exactly_one_failed_terminal() {
    let captured = Recorder::new();
    let (_tx, body) = channel();
    let control = TurnControl {
        deadline: Some(Instant::now() + DEADLINE),
        ..observing(&captured)
    };
    let (turn, _) = open(body, control);
    let result = tokio::time::timeout(Duration::from_secs(2), turn.finish())
        .await
        .unwrap();
    assert_eq!(result.cause, Some(FailureCause::Deadline));
    let failed = only(captured.ended());
    assert_eq!(failed.cause.map(FailureCause::as_str), Some("deadline"));
    assert_eq!(failed.elapsed, DEADLINE);
    assert_eq!(captured.measured().len(), 0);
}

#[tokio::test]
async fn dropping_an_unfinished_turn_emits_one_cancelled_terminal() {
    let captured = Recorder::new();
    let (turn, physical) = open(Box::pin(stream::pending()), observing(&captured));
    drop(turn);
    assert_eq!(
        captured
            .seen()
            .iter()
            .filter(|seen| **seen == Seen::Started)
            .count(),
        1
    );
    assert_eq!(
        only(captured.ended()).cause.map(FailureCause::as_str),
        Some("cancelled")
    );
    assert_eq!(physical.sends.load(Ordering::Relaxed), 0);
}

#[tokio::test(start_paused = true)]
async fn empty_and_later_chunks_cannot_duplicate_the_first_text_measurement() {
    let captured = Recorder::new();
    let mut trace = TurnTrace::start("model", Some(Arc::<Recorder>::clone(&captured)));
    trace.text(0);
    assert_eq!(captured.first_texts().len(), 0);
    tokio::time::advance(Duration::from_millis(25)).await;
    trace.text(3);
    tokio::time::advance(Duration::from_millis(25)).await;
    trace.text(4);
    trace.end(None);
    drop(trace);
    assert_eq!(captured.first_texts(), [Duration::from_millis(25)]);
    assert_eq!(only(captured.ended()).text_bytes, 7);
}

#[tokio::test(start_paused = true)]
async fn observer_excludes_recovery_and_preserves_the_provider_terminal() {
    let captured = Recorder::new();
    let mut trace = TurnTrace::start("model", Some(Arc::<Recorder>::clone(&captured)));
    trace.text(0);
    tokio::time::advance(Duration::from_millis(25)).await;
    trace.text(1);
    tokio::time::advance(Duration::from_millis(25)).await;
    trace.terminal(Some(FailureCause::Provider));
    tokio::time::advance(Duration::from_secs(5)).await;
    trace.end(Some(FailureCause::Provider));
    drop(trace);
    assert_eq!(
        captured.ended(),
        vec![TurnObservation {
            tokens: ObservedTokens::default(),
            elapsed: Duration::from_millis(50),
            first_text: Some(Duration::from_millis(25)),
            cause: Some(FailureCause::Provider),
            text_bytes: 1,
        }]
    );
}

#[tokio::test(start_paused = true)]
async fn dropping_a_failed_stream_keeps_its_original_outcome() {
    let captured = Recorder::new();
    let body = stream::once(async { Err(reset("fixture transport interruption")) });
    let (mut turn, _) = open(Box::pin(body), observing(&captured));
    assert!(matches!(
        turn.next().await,
        Some(Err(FailureCause::Provider))
    ));
    tokio::time::advance(Duration::from_secs(5)).await;
    drop(turn);
    let ended = only(captured.ended());
    assert_eq!(ended.cause, Some(FailureCause::Provider));
    assert_eq!(ended.elapsed, Duration::ZERO);
    assert_eq!(ended.first_text, None);
}

#[tokio::test]
async fn a_call_without_text_has_no_first_token_sample() {
    let captured = Recorder::new();
    let wire = format!("{USAGE}{STOP}");
    let (turn, _) = open(
        Box::pin(stream::once(async move { Ok(Bytes::from(wire)) })),
        observing(&captured),
    );
    assert!(turn.finish().await.failure.is_none());
    let ended = only(captured.ended());
    assert_eq!(ended.cause, None);
    assert_eq!(ended.first_text, None);
}

#[tokio::test]
async fn dropping_a_pending_call_notifies_cancellation_once() {
    let captured = Recorder::new();
    let (turn, _) = open(Box::pin(stream::pending()), observing(&captured));
    assert!(format!("{:?}", observing(&captured)).contains("TurnControl"));
    drop(turn);
    assert_eq!(only(captured.ended()).cause, Some(FailureCause::Cancelled));
}

#[tokio::test(start_paused = true)]
async fn dropping_after_eof_preserves_the_completed_terminal() {
    let captured = Recorder::new();
    let wire = format!("{TEXT}{USAGE}{STOP}");
    let (mut turn, _) = open(
        Box::pin(stream::once(async move { Ok(Bytes::from(wire)) })),
        observing(&captured),
    );
    while turn.next().await.is_some() {}
    tokio::time::advance(Duration::from_secs(2)).await;
    drop(turn);
    let ended = only(captured.ended());
    assert_eq!(ended.cause, None);
    assert_eq!(ended.elapsed, Duration::ZERO);
    assert_eq!(ended.tokens.input, Some(3));
    assert_eq!(ended.tokens.output, Some(2));
}
