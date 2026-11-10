#![expect(
    clippy::expect_used,
    reason = "invalid fixture identifiers must fail loudly"
)]

use super::finished;
use af_core::{error_code, test_util::trace::Capture};
use tokio_util::sync::CancellationToken;

#[test]
fn an_unexpected_component_failure_keeps_its_registered_code_and_cause() {
    let logs = Capture::install();
    let stop = CancellationToken::new();
    let failed = CancellationToken::new();
    let error =
        af_core::id::Uuid7::parse("malformed").expect_err("invalid fixture identifier refused");
    finished(&Err(error), "scheduler", &stop, &failed);
    let line = logs.only("runner_stream_component_failed");
    assert_eq!(
        line.field("error_code"),
        Some(error_code::UUIDV7_INVALID_ID_SHAPE.as_str())
    );
    assert_eq!(line.field("outcome"), Some("failed"));
    assert!(
        line.field("reason")
            .is_some_and(|reason| reason.contains("identifier") && !reason.contains("[UZ-"))
    );
    assert!(failed.is_cancelled());
}

#[test]
fn a_native_io_failure_keeps_the_typed_network_cause_in_its_log() {
    let logs = Capture::install();
    let failed = CancellationToken::new();
    let error = af_stream::Error::from(std::io::Error::from(std::io::ErrorKind::ConnectionReset));
    finished(&Err(error), "listener", &CancellationToken::new(), &failed);
    let line = logs.only("runner_stream_component_failed");
    assert_eq!(
        line.field("error_code"),
        Some(error_code::INTERNAL_OPERATION_FAILED.as_str())
    );
    assert!(
        line.field("reason")
            .is_some_and(|reason| reason.contains("connection reset"))
    );
    assert!(failed.is_cancelled());
}

#[test]
fn unexpected_success_still_fails_and_expected_shutdown_does_not() {
    let logs = Capture::install();
    let stop = CancellationToken::new();
    let failed = CancellationToken::new();
    finished(&Ok::<(), af_stream::Error>(()), "listener", &stop, &failed);
    assert!(failed.is_cancelled());
    let line = logs.only("runner_stream_component_failed");
    assert_eq!(line.field("outcome"), Some("unexpected_return"));
    assert_eq!(
        line.field("reason"),
        Some("the native component returned before shutdown")
    );
    stop.cancel();
    let expected = CancellationToken::new();
    finished(
        &Ok::<(), af_stream::Error>(()),
        "listener",
        &stop,
        &expected,
    );
    assert!(!expected.is_cancelled());
    assert_eq!(
        logs.only("runner_stream_component_stopped")
            .field("component"),
        Some("listener")
    );
}

#[test]
fn a_failure_during_drain_keeps_its_cause_without_failing_the_daemon() {
    let logs = Capture::install();
    let stop = CancellationToken::new();
    stop.cancel();
    let failed = CancellationToken::new();
    let error = af_stream::Error::from(std::io::Error::from(std::io::ErrorKind::ConnectionReset));
    finished(&Err(error), "listener", &stop, &failed);
    assert!(!failed.is_cancelled(), "drain decides the exit status");
    let line = logs.only("runner_stream_component_stopped");
    assert_eq!(
        line.field("error_code"),
        Some(error_code::INTERNAL_OPERATION_FAILED.as_str())
    );
    assert!(
        line.field("reason")
            .is_some_and(|reason| reason.contains("connection reset"))
    );
}
