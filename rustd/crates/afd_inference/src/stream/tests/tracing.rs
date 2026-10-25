//! Correlated timing without prompts, response bodies or credentials.

use super::*;
use afd_core::test_util::trace::Capture;
use tracing_subscriber::filter::LevelFilter;

const CORRELATION: &str = "dispatch-timing-fixture";
const DEADLINE_MILLIS: &str = "1000";

#[tokio::test]
async fn debug_rendering_excludes_private_request_and_response_content() {
    let wire = format!("{TEXT}{USAGE}{STOP}");
    let (turn, _) = open(
        Box::pin(stream::once(async move { Ok(Bytes::from(wire)) })),
        control(),
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

fn control() -> TurnControl {
    TurnControl {
        correlation_id: Arc::from(CORRELATION),
        ..Default::default()
    }
}

#[tokio::test]
async fn completed_stream_has_one_timing_pair_and_content_free_first_text() {
    let captured = Capture::install_filtered(crate::logs::log_filter(LevelFilter::TRACE));
    let wire = format!("{TEXT}{USAGE}{STOP}");
    let (turn, _) = open(
        Box::pin(stream::once(async move { Ok(Bytes::from(wire)) })),
        control(),
    );
    assert!(turn.finish().await.failure.is_none());
    for name in [
        "model_turn_started",
        "model_turn_first_text",
        "model_turn_completed",
    ] {
        assert_eq!(
            captured.only(name).field("correlation_id"),
            Some(CORRELATION)
        );
    }
    let first = captured.only("model_turn_first_text");
    assert_eq!(first.level, ::tracing::Level::DEBUG);
    assert!(first.field("duration_ms").is_some());
    assert_eq!(
        captured.only("model_turn_completed").field("text_bytes"),
        Some("7")
    );
    let fields = format!("{:?}", captured.events());
    let calibration = captured.only("model_input_calibration");
    assert_eq!(calibration.field("actual_input_tokens"), Some("3"));
    assert_eq!(calibration.field("method"), Some("utf8_estimate"));
    assert!(calibration.field("measured_input_tokens").is_some());
    assert!(calibration.field("revision").is_some());
    assert_eq!(calibration.field("route_id").map(str::len), Some(64));
    for private in ["private prompt", "private-credential", "Hi 🌿"] {
        assert!(
            !fields.contains(private),
            "private provider content reached tracing"
        );
    }
    assert!(
        !captured
            .events()
            .iter()
            .any(|event| event.field("event") == Some("model_turn_failed"))
    );
}

#[tokio::test(start_paused = true)]
async fn deadline_has_exactly_one_failed_terminal_with_a_registry_code() {
    let captured = Capture::install();
    let (_tx, body) = channel();
    let control = TurnControl {
        deadline: Some(Instant::now() + Duration::from_secs(1)),
        ..control()
    };
    let (turn, _) = open(body, control);
    let result = tokio::time::timeout(Duration::from_secs(2), turn.finish())
        .await
        .unwrap();
    assert_eq!(result.cause, Some(FailureCause::Deadline));
    let failed = captured.only("model_turn_failed");
    assert_eq!(failed.field("correlation_id"), Some(CORRELATION));
    assert_eq!(failed.field("reason"), Some("deadline"));
    assert_eq!(failed.field("duration_ms"), Some(DEADLINE_MILLIS));
    assert!(
        !captured
            .events()
            .iter()
            .any(|event| event.field("event") == Some("model_input_calibration"))
    );
    assert_eq!(
        failed.field("error_code"),
        Some(afd_core::error_code::INTERNAL_OPERATION_FAILED.as_str())
    );
    assert!(
        !captured
            .events()
            .iter()
            .any(|event| event.field("event") == Some("model_turn_completed"))
    );
}

#[tokio::test]
async fn dropping_an_unfinished_turn_emits_one_cancelled_terminal() {
    let captured = Capture::install();
    let (turn, physical) = open(Box::pin(stream::pending()), control());
    drop(turn);
    assert_eq!(
        captured.only("model_turn_started").field("correlation_id"),
        Some(CORRELATION)
    );
    assert_eq!(
        captured.only("model_turn_failed").field("reason"),
        Some("cancelled")
    );
    assert_eq!(physical.sends.load(Ordering::Relaxed), 0);
}

#[tokio::test(start_paused = true)]
async fn empty_and_later_chunks_cannot_duplicate_the_first_text_measurement() {
    let captured = Capture::install_filtered(crate::logs::log_filter(LevelFilter::TRACE));
    let mut trace = super::super::trace::TurnTrace::start(Arc::from(CORRELATION));
    trace.text(0);
    assert!(
        !captured
            .events()
            .iter()
            .any(|event| { event.field("event") == Some("model_turn_first_text") })
    );
    tokio::time::advance(Duration::from_millis(25)).await;
    trace.text(3);
    tokio::time::advance(Duration::from_millis(25)).await;
    trace.text(4);
    trace.end(None);
    drop(trace);
    assert_eq!(
        captured.only("model_turn_first_text").field("duration_ms"),
        Some("25")
    );
    assert_eq!(
        captured.only("model_turn_completed").field("text_bytes"),
        Some("7")
    );
}
