//! The one line every failure the loop continues past is logged through.

use afd_core::test_util::trace::Capture;

use super::report;
use crate::error::query;

/// The event the fixture failure is reported under.
const EVENT: &str = "outbound_fixture_failed";

/// The line carries the registry code in its own field, so the `reason` beside
/// it is the failure's sentence and its causes — never the code a second time,
/// and never a captured backtrace.
#[test]
fn a_logged_failure_reason_carries_no_code_prefix() {
    let capture = Capture::install();
    let failure = query("recording the fixture")(sqlx::Error::PoolClosed);

    report(EVENT, &failure);

    let logged = capture.only(EVENT);
    assert_eq!(logged.field("error_code"), Some(failure.code().as_str()));
    logged.assert_uncoded_reason();
    let reason = logged.field("reason").unwrap_or_default();
    assert!(
        reason.starts_with("statement failed during recording the fixture"),
        "the reason opens with the failure's own sentence: {reason}"
    );
}
