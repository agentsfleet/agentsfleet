//! A run's lines: correlated by its id, content-free, a failure under a
//! registry code. The timings they report are guffbolt-ai's
//! (`guffbolt_ai::wire::stream`).

use std::time::Duration;

use af_core::test_util::trace::Capture;
use guffbolt_ai::wire::{
    Calibration, FailureCause, ObservedTokens, Observer as _, Retrying, TurnObservation,
};
use tracing_subscriber::filter::LevelFilter;

use super::RunObserver;

const CORRELATION: &str = "dispatch-timing-fixture";
const DEADLINE_MILLIS: &str = "1000";

fn observer() -> RunObserver {
    RunObserver::new(CORRELATION, "fake")
}

fn ended(cause: Option<FailureCause>, elapsed: Duration) -> TurnObservation {
    TurnObservation {
        tokens: ObservedTokens::default(),
        elapsed,
        first_text: None,
        cause,
        text_bytes: 7,
    }
}

#[test]
fn a_run_observer_prints_whose_turns_it_watches() {
    assert_eq!(
        format!("{:?}", RunObserver::new("run-1", "fake")),
        r#"RunObserver { run_id: "run-1", provider: "fake", .. }"#
    );
}

#[test]
fn completed_stream_has_one_timing_pair_and_content_free_first_text() {
    let captured = Capture::install_filtered(af_inference::log_filter(LevelFilter::TRACE));
    let observer = observer();
    observer.turn_started();
    observer.first_text(Duration::from_millis(25));
    observer.input_measured(&Calibration {
        route_id: &"a".repeat(64),
        model_id: &"b".repeat(64),
        provider_api: "chat",
        request_kind: "messages",
        method: "o200k_proxy_estimate",
        revision: "rig-0.45.0:tiktoken-rs-0.12.1:request-v3",
        measured_input_tokens: 4,
        actual_input_tokens: 3,
        measure_us: 11,
    });
    observer.turn_ended("model", &ended(None, Duration::from_millis(50)));
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
    assert_eq!(first.field("duration_ms"), Some("25"));
    assert_eq!(
        captured.only("model_turn_completed").field("text_bytes"),
        Some("7")
    );
    let calibration = captured.only("model_input_calibration");
    assert_eq!(calibration.field("actual_input_tokens"), Some("3"));
    assert_eq!(calibration.field("method"), Some("o200k_proxy_estimate"));
    assert_eq!(calibration.field("correlation_id"), Some(CORRELATION));
    assert!(calibration.field("measured_input_tokens").is_some());
    assert!(calibration.field("revision").is_some());
    assert!(
        calibration
            .field("measure_us")
            .is_some_and(|micros| micros.parse::<u64>().is_ok()),
        "the calibration names how long the local count took"
    );
    assert_eq!(calibration.field("route_id").map(str::len), Some(64));
    assert!(
        !captured
            .events()
            .iter()
            .any(|event| event.field("event") == Some("model_turn_failed"))
    );
}

#[test]
fn deadline_has_exactly_one_failed_terminal_with_a_registry_code() {
    let captured = Capture::install();
    observer().turn_ended(
        "model",
        &ended(Some(FailureCause::Deadline), Duration::from_secs(1)),
    );
    let failed = captured.only("model_turn_failed");
    assert_eq!(failed.field("correlation_id"), Some(CORRELATION));
    assert_eq!(failed.field("reason"), Some("deadline"));
    assert_eq!(failed.field("duration_ms"), Some(DEADLINE_MILLIS));
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

#[test]
fn a_retried_send_names_its_run_and_the_providers_host_alone() {
    let captured = Capture::install();
    RunObserver::new("run-1", "custom:https://user:s3cret@vllm.corp/v1").retrying(Retrying {
        attempt: 1,
        status: Some(503),
        wait: Duration::from_millis(1500),
    });
    let retry = captured.only("provider_retry");
    assert_eq!(retry.field("run_id"), Some("run-1"));
    assert_eq!(retry.field("provider"), Some("custom:vllm.corp"));
    assert_eq!(retry.field("status"), Some("503"));
    assert_eq!(retry.field("wait_ms"), Some("1500"));
    assert_eq!(
        retry.field("error_code"),
        Some(af_core::error_code::INTERNAL_OPERATION_FAILED.as_str())
    );
    assert!(!format!("{:?}", captured.events()).contains("s3cret"));
}
