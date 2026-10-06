#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
)]

use afd_core::test_util::trace::Capture;
use rustix::process::{Pid, Signal};
use serde_json::Value;
use tracing::Level;

use super::{EVENT_PROCESS_COMPLETED, EVENT_PROCESS_FAILED, EVENT_SIGNAL_MISSED, Group, report};
use crate::api::Ending;

#[test]
fn an_ending_with_no_status_is_logged_failed_and_any_other_completed() {
    let capture = Capture::install();

    report(3, Ending::Interrupted);
    report(4, Ending::Exited(0));

    let failed = capture.only(EVENT_PROCESS_FAILED);
    let completed = capture.only(EVENT_PROCESS_COMPLETED);
    assert_eq!(failed.level, Level::WARN);
    assert_eq!(failed.field("error_code"), Some("UZ-INTERNAL-003"));
    assert_eq!(failed.field("process_id"), Some("3"));
    assert_eq!(completed.level, Level::DEBUG);
    assert_eq!(completed.field("ending"), Some("exited"));
    assert_eq!(completed.field("code"), Some("0"));
}

#[test]
fn an_ending_is_logged_under_the_kind_the_wire_spells() {
    for ending in [
        Ending::Exited(2),
        Ending::Signaled(9),
        Ending::TimedOut,
        Ending::Interrupted,
    ] {
        let wire = serde_json::to_value(ending).unwrap();

        assert_eq!(wire["kind"], ending.kind(), "{ending:?}");
        assert_eq!(
            wire.get("code").and_then(Value::as_i64),
            ending.code().map(i64::from),
            "{ending:?}"
        );
    }
}

#[test]
fn a_signal_to_a_group_already_gone_is_logged_with_its_process() {
    let capture = Capture::install();
    // Past every platform's process-number range, so never a live group.
    let group = Group {
        process: 5,
        pid: Pid::from_raw(i32::MAX).unwrap(),
    };

    group.signal(Signal::TERM);

    let missed = capture.only(EVENT_SIGNAL_MISSED);
    assert_eq!(missed.field("process_id"), Some("5"));
    assert!(missed.field("reason").is_some());
}
