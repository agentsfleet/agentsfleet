//! `schedule`: a moment, written as the UTC minute it fires at, once.

#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_core::clock::{self, UnixMillis};
use afd_core::timing::DAY_MS;
use jiff::Timestamp;
use serde_json::json;

use super::{DETAIL_AT, DETAIL_PAST, DETAIL_TOO_FAR, ScheduleOnce, minute_of};
use crate::handler::Typed;
use crate::runtime::ToolErrorCode;
use crate::testing::{Asked, OwnedCall, RecordingVerbs, call, lease_with};

/// The instant every pure case is measured from: 2026-10-05T00:00:00Z.
const NOW: UnixMillis = UnixMillis::from_millis(1_791_158_400_000);

#[test]
fn an_offset_moment_is_written_as_its_utc_minute() {
    // 09:30 in Kolkata is 04:00 UTC.
    assert_eq!(
        minute_of("2026-10-06T09:30:00+05:30", NOW),
        Ok("0 4 6 10 *".to_owned())
    );
}

#[test]
fn a_moment_already_gone_is_refused() {
    assert_eq!(minute_of("2026-10-04T09:00:00Z", NOW), Err(DETAIL_PAST));
}

/// A moment inside a minute fires at the next minute, never before it.
#[test]
fn a_moment_inside_a_minute_rounds_up() {
    assert_eq!(
        minute_of("2026-10-06T09:00:30Z", NOW),
        Ok("1 9 6 10 *".to_owned())
    );
}

/// A minute already begun, or about to, could not be registered in time, and
/// its expression's next match is a year away: refused, not deferred.
#[test]
fn a_moment_inside_the_lead_is_refused() {
    // NOW is midnight; 00:00:30 rounds to 00:01, one minute out: allowed.
    assert_eq!(
        minute_of("2026-10-05T00:00:30Z", NOW),
        Ok("1 0 5 10 *".to_owned())
    );
    // NOW itself, and anything rounding to it, is too close.
    assert_eq!(minute_of("2026-10-05T00:00:00Z", NOW), Err(DETAIL_PAST));
    let just_after = UnixMillis::from_millis(NOW.as_millis() + 1);
    assert_eq!(
        minute_of("2026-10-05T00:01:00Z", just_after),
        Err(DETAIL_PAST),
        "a rounded minute less than a minute out"
    );
}

/// A cron has no year: past the horizon the expression would fire early.
#[test]
fn a_moment_past_the_horizon_is_refused() {
    assert_eq!(minute_of("2027-10-06T00:00:00Z", NOW), Err(DETAIL_TOO_FAR));
}

#[test]
fn an_unreadable_moment_is_refused() {
    assert_eq!(minute_of("next tuesday", NOW), Err(DETAIL_AT));
}

#[tokio::test]
async fn test_schedule_tool_is_once() {
    let tomorrow = Timestamp::from_millisecond(clock::now().as_millis() + DAY_MS)
        .expect("tomorrow is an instant")
        .to_string();
    let verbs = RecordingVerbs::answering(Ok("{}".to_owned()), Ok(true));
    let lease = lease_with(&verbs);
    let output = call(
        Typed::boxed(ScheduleOnce).as_ref(),
        &lease,
        json!({"at": tomorrow, "message": "re-check the error rate"}),
    )
    .await;
    assert_eq!(output.error_code, None, "{}", output.text);
    let asked = verbs.asked();
    let [
        Asked::Schedules(OwnedCall::Create {
            timezone,
            once,
            message,
            ..
        }),
    ] = asked.as_slice()
    else {
        panic!("one create, not {asked:?}");
    };
    assert!(*once, "a one-off retires after it fires");
    assert_eq!(timezone.as_deref(), Some("UTC"));
    assert_eq!(message, "re-check the error rate");
}

#[tokio::test]
async fn a_refused_moment_sends_nothing() {
    let verbs = RecordingVerbs::answering(Ok("{}".to_owned()), Ok(true));
    let lease = lease_with(&verbs);
    let output = call(
        Typed::boxed(ScheduleOnce).as_ref(),
        &lease,
        json!({"at": "2000-01-01T00:00:00Z", "message": "too late"}),
    )
    .await;
    assert_eq!(output.error_code, Some(ToolErrorCode::InvalidArguments));
    assert!(verbs.asked().is_empty());
}
