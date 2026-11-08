//! Correlated timing without prompts, response bodies or credentials.

use super::*;
use af_core::test_util::trace::Capture;
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
    assert_eq!(calibration.field("method"), Some("o200k_proxy_estimate"));
    assert!(calibration.field("measured_input_tokens").is_some());
    assert!(calibration.field("revision").is_some());
    assert!(
        calibration
            .field("measure_us")
            .is_some_and(|micros| micros.parse::<u64>().is_ok()),
        "the calibration names how long the local count took"
    );
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
        Some(af_core::error_code::INTERNAL_OPERATION_FAILED.as_str())
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
    let mut trace = TurnTrace::start(Arc::from(CORRELATION), None);
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

fn observed() -> (Arc<Mutex<Vec<TurnObservation>>>, TurnObserver) {
    let values = Arc::new(Mutex::new(Vec::new()));
    let stored = Arc::clone(&values);
    (
        values,
        Arc::new(move |value| stored.lock().unwrap().push(value)),
    )
}

#[tokio::test(start_paused = true)]
async fn observer_excludes_recovery_and_preserves_the_provider_terminal() {
    let (values, observer) = observed();
    let mut trace = TurnTrace::start(Arc::from(CORRELATION), Some(observer));
    trace.text(0);
    tokio::time::advance(Duration::from_millis(25)).await;
    trace.text(1);
    tokio::time::advance(Duration::from_millis(25)).await;
    trace.terminal(Some(FailureCause::Provider));
    tokio::time::advance(Duration::from_secs(5)).await;
    trace.end(Some(FailureCause::Provider));
    drop(trace);
    assert_eq!(
        *values.lock().unwrap(),
        vec![TurnObservation {
            tokens: af_wire::usage::ObservedTokens::default(),
            elapsed: Duration::from_millis(50),
            first_text: Some(Duration::from_millis(25)),
            cause: Some(FailureCause::Provider),
        }]
    );
}

#[tokio::test(start_paused = true)]
async fn dropping_a_failed_stream_keeps_its_original_outcome() {
    let (values, observer) = observed();
    let body = stream::once(async {
        Err(rig_http::Error::instance(std::io::Error::other(
            "fixture transport interruption",
        )))
    });
    let (mut turn, _) = open(
        Box::pin(body),
        TurnControl {
            observer: Some(observer),
            ..control()
        },
    );
    assert!(matches!(
        turn.next().await,
        Some(Err(FailureCause::Provider))
    ));
    tokio::time::advance(Duration::from_secs(5)).await;
    drop(turn);
    let values = values.lock().unwrap();
    assert_eq!(values.len(), 1);
    assert_eq!(values.first().unwrap().cause, Some(FailureCause::Provider));
    assert_eq!(values.first().unwrap().elapsed, Duration::ZERO);
    assert_eq!(values.first().unwrap().first_text, None);
}

#[tokio::test]
async fn a_call_without_text_has_no_first_token_sample() {
    let (values, observer) = observed();
    let wire = format!("{USAGE}{STOP}");
    let (turn, _) = open(
        Box::pin(stream::once(async move { Ok(Bytes::from(wire)) })),
        TurnControl {
            observer: Some(observer),
            ..control()
        },
    );
    assert!(turn.finish().await.failure.is_none());
    let values = values.lock().unwrap();
    assert_eq!(values.len(), 1);
    assert_eq!(values.first().unwrap().cause, None);
    assert_eq!(values.first().unwrap().first_text, None);
}

#[tokio::test]
async fn dropping_a_pending_call_notifies_cancellation_once() {
    let (values, observer) = observed();
    let (turn, _) = open(
        Box::pin(stream::pending()),
        TurnControl {
            observer: Some(observer),
            ..control()
        },
    );
    assert!(format!("{:?}", control()).contains("TurnControl"));
    drop(turn);
    let values = values.lock().unwrap();
    assert_eq!(values.len(), 1);
    assert_eq!(values.first().unwrap().cause, Some(FailureCause::Cancelled));
}

#[tokio::test(start_paused = true)]
async fn dropping_after_eof_preserves_the_completed_terminal() {
    let (values, observer) = observed();
    let wire = format!("{TEXT}{USAGE}{STOP}");
    let (mut turn, _) = open(
        Box::pin(stream::once(async move { Ok(Bytes::from(wire)) })),
        TurnControl {
            observer: Some(observer),
            ..control()
        },
    );
    while turn.next().await.is_some() {}
    tokio::time::advance(Duration::from_secs(2)).await;
    drop(turn);
    let values = values.lock().unwrap();
    assert_eq!(values.len(), 1);
    assert_eq!(values.first().unwrap().cause, None);
    assert_eq!(values.first().unwrap().elapsed, Duration::ZERO);
    assert_eq!(values.first().unwrap().tokens.input, Some(3));
    assert_eq!(values.first().unwrap().tokens.output, Some(2));
}
