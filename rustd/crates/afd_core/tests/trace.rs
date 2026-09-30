//! The `tracing` capture every crate's suites assert log lines through: what it
//! keeps of an event, and that two captures never overlap.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use afd_core::test_util::trace::Capture;
use tracing::Level;

const EVENT: &str = "capture_probe";
const FLEET: &str = "fleet_probe";
const ATTEMPTS: u64 = 3;
/// Long enough that a second capture free to start would have started.
const HOLD: Duration = Duration::from_millis(200);
/// How long the second capture may take once the first has ended.
const RELEASE: Duration = Duration::from_secs(10);

#[test]
fn trace_capture_records_fields() {
    let capture = Capture::install();
    tracing::warn!(
        event = EVENT,
        fleet = FLEET,
        attempts = ATTEMPTS,
        "probe raised"
    );

    let seen = capture.only(EVENT);
    assert_eq!(seen.level, Level::WARN);
    // A string field arrives as written, not as its quoted `Debug` form.
    assert_eq!(seen.fields.get("fleet").map(String::as_str), Some(FLEET));
    assert_eq!(seen.fields.get("attempts").map(String::as_str), Some("3"));
    assert_eq!(
        seen.fields.get("message").map(String::as_str),
        Some("probe raised")
    );
    assert_eq!(capture.events().len(), 1, "one event raised, one captured");
}

#[test]
fn trace_capture_serialises_concurrent_tests() {
    let first = Capture::install();
    let (entered, started) = mpsc::channel();
    let second = thread::spawn(move || {
        let capture = Capture::install();
        entered.send(()).expect("the test thread is still waiting");
        tracing::info!(event = EVENT);
        capture.events().len()
    });

    assert!(
        started.recv_timeout(HOLD).is_err(),
        "a second capture started while the first was live"
    );
    drop(first);
    started
        .recv_timeout(RELEASE)
        .expect("the second capture starts once the first ends");
    // Its own thread's event, and nothing the first capture's thread raised.
    assert_eq!(second.join().expect("the second thread finishes"), 1);
}
