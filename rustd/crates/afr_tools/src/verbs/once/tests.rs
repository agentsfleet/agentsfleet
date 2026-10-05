//! `schedule`: a moment, written as the UTC minute it fires at, once.

use afd_core::clock::{self, UnixMillis};
use afd_core::timing::DAY_MS;
use jiff::Timestamp;
use serde_json::json;

use super::{DETAIL_AT, DETAIL_PAST, DETAIL_TOO_FAR, ScheduleOnce, minute_of};
use crate::handler::Typed;
use crate::runtime::ToolErrorCode;
use crate::testing::{Asked, RecordingVerbs, call, lease_with};

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
    let mut lease = lease_with(&verbs);
    let output = call(
        Typed::boxed(ScheduleOnce).as_ref(),
        &mut lease,
        json!({"at": tomorrow, "message": "re-check the error rate"}),
    )
    .await;
    assert_eq!(output.error_code, None, "{}", output.text);
    let asked = verbs.asked();
    let [Asked::Schedules(created)] = asked.as_slice() else {
        panic!("one schedules call, not {asked:?}");
    };
    assert!(
        created.starts_with("Create") && created.contains("once: true"),
        "{created}"
    );
    assert!(created.contains(r#"timezone: Some("UTC")"#), "{created}");
}

#[tokio::test]
async fn a_refused_moment_sends_nothing() {
    let verbs = RecordingVerbs::answering(Ok("{}".to_owned()), Ok(true));
    let mut lease = lease_with(&verbs);
    let output = call(
        Typed::boxed(ScheduleOnce).as_ref(),
        &mut lease,
        json!({"at": "2000-01-01T00:00:00Z", "message": "too late"}),
    )
    .await;
    assert_eq!(output.error_code, Some(ToolErrorCode::InvalidArguments));
    assert!(verbs.asked().is_empty());
}
