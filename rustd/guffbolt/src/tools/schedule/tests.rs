//! The schedule tools over a scheduler that records what they asked: each
//! onto its call, the call's place handed on, and the scheduler's words read
//! back as they came.

#![expect(
    clippy::expect_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::time::Duration;

use jiff::Timestamp;
use serde_json::{Value, json};

use super::once::minute_of;
use crate::tools::fake::{Asked, Fake};
use crate::tools::outbound::Allowlist;
use crate::tools::{Local, Refusal, ToolsConfig, code};
use crate::{CallRef, Tools};

/// The instant every pure case is measured from: 2026-10-05T00:00:00Z.
const NOW: Timestamp = Timestamp::constant(1_791_158_400, 0);
/// The finest step an instant on the wire takes.
const ONE_MILLISECOND: Duration = Duration::from_millis(1);
/// A day.
const DAY: Duration = Duration::from_hours(24);
/// What the fake scheduler answers every call with.
const ANSWER: &str = r#"{"schedule_id":"job-1"}"#;

/// Where the call each case makes sits.
fn at() -> CallRef {
    CallRef {
        step: 3,
        index: 1,
        call_id: "call-3-1".to_owned(),
    }
}

/// Calls the tool `name` over `fake` with `arguments`.
async fn called(fake: &Fake, name: &str, arguments: Value) -> crate::ToolOutput {
    let local = Local::new("run-1", std::env::temp_dir(), Allowlist::Any)
        .expect("the temporary directory exists");
    let tools = Tools::builder()
        .with_scheduler(fake)
        .build(&ToolsConfig::new([name]).expect("a name"))
        .expect("the scheduler is given");
    let at = at();
    tools.dispatch(name, &arguments, local.context(&at)).await
}

#[tokio::test]
async fn each_cron_tool_asks_its_scheduler_and_reads_its_words() {
    let fake = Fake::answering(Ok(ANSWER.to_owned()));
    let cases = [
        (
            "cron_add",
            json!({"cron": "0 9 * * 1", "timezone": "Asia/Kolkata", "message": "weekly check"}),
        ),
        ("cron_list", json!({})),
        (
            "cron_update",
            json!({"schedule_id": "job-1", "message": "daily check", "paused": true}),
        ),
        ("cron_remove", json!({"schedule_id": "job-1"})),
        ("cron_run", json!({"schedule_id": "job-1"})),
        (
            "cron_runs",
            json!({"schedule_id": "job-1", "limit": 2, "starting_after": "c"}),
        ),
    ];

    for (name, arguments) in cases {
        let output = called(&fake, name, arguments).await;
        assert_eq!(
            (output.text.as_str(), output.error_code),
            (ANSWER, None),
            "{name}"
        );
    }

    assert_eq!(
        fake.asked(),
        [
            Asked::Add(
                at(),
                "0 9 * * 1".to_owned(),
                Some("Asia/Kolkata".to_owned()),
                "weekly check".to_owned(),
                false
            ),
            Asked::List,
            Asked::Update(
                "job-1".to_owned(),
                None,
                None,
                Some("daily check".to_owned()),
                Some(true)
            ),
            Asked::Remove("job-1".to_owned()),
            Asked::RunNow("job-1".to_owned()),
            Asked::Runs("job-1".to_owned(), Some(2), Some("c".to_owned())),
        ]
    );
}

#[tokio::test]
async fn an_empty_answer_reads_done_and_a_refusal_keeps_its_code() {
    let done = called(
        &Fake::answering(Ok(String::new())),
        "cron_remove",
        json!({"schedule_id": "job-1"}),
    )
    .await;
    let refused = called(
        &Fake::answering(Err(Refusal::new(
            code::SCHEDULE_NOT_FOUND,
            "no schedule has the id x",
        ))),
        "cron_run",
        json!({"schedule_id": "x"}),
    )
    .await;

    assert_eq!((done.text.as_str(), done.error_code), ("done", None));
    assert_eq!(refused.error_code, Some(code::SCHEDULE_NOT_FOUND));
    assert_eq!(
        refused.text,
        "[schedule_not_found] no schedule has the id x"
    );
}

#[tokio::test]
async fn arguments_outside_a_tools_schema_ask_nothing() {
    let fake = Fake::answering(Ok(ANSWER.to_owned()));
    let cases = [
        ("cron_add", json!({"cron": "0 9 * * 1"})),
        (
            "cron_add",
            json!({"cron": "0 9 * * 1", "message": "m", "fleet_id": "x"}),
        ),
        ("cron_list", json!({"all": true})),
        ("cron_update", json!({"paused": true})),
        ("cron_runs", json!({"schedule_id": "job-1", "limit": -1})),
        ("schedule", json!({"message": "m"})),
    ];

    for (name, arguments) in cases {
        let output = called(&fake, name, arguments.clone()).await;
        assert_eq!(
            output.error_code,
            Some(code::INVALID_ARGUMENTS),
            "{name} {arguments}"
        );
    }
    assert_eq!(fake.asked(), []);
}

#[tokio::test]
async fn schedule_adds_a_one_off_at_its_utc_minute() {
    let fake = Fake::answering(Ok(ANSWER.to_owned()));
    let tomorrow = Timestamp::now()
        .saturating_add(DAY)
        .expect("a day from now is in range");

    let output = called(
        &fake,
        "schedule",
        json!({"at": tomorrow.to_string(), "message": "re-check the error rate"}),
    )
    .await;

    assert_eq!(output.error_code, None, "{}", output.text);
    let asked = fake.asked();
    let [Asked::Add(call, _cron, timezone, message, once)] = asked.as_slice() else {
        unreachable!("one add, not {asked:?}");
    };
    assert_eq!(call, &at());
    assert_eq!(
        (timezone.as_deref(), message.as_str(), *once),
        (Some("UTC"), "re-check the error rate", true)
    );
}

#[tokio::test]
async fn a_moment_schedule_refuses_asks_nothing() {
    let fake = Fake::answering(Ok(ANSWER.to_owned()));

    let output = called(
        &fake,
        "schedule",
        json!({"at": "2000-01-01T00:00:00Z", "message": "too late"}),
    )
    .await;

    assert_eq!(output.error_code, Some(code::INVALID_ARGUMENTS));
    assert_eq!(fake.asked(), []);
}

#[test]
fn an_offset_moment_is_written_as_its_utc_minute() {
    // 09:30 in Kolkata is 04:00 UTC.
    assert_eq!(
        minute_of("2026-10-06T09:30:00+05:30", NOW),
        Ok("0 4 6 10 *".to_owned())
    );
}

#[test]
fn a_moment_inside_a_minute_rounds_up_to_the_next() {
    assert_eq!(
        minute_of("2026-10-06T09:00:30Z", NOW),
        Ok("1 9 6 10 *".to_owned())
    );
    assert_eq!(
        minute_of("2026-10-06T09:00:00.001Z", NOW),
        Ok("1 9 6 10 *".to_owned())
    );
    assert_eq!(
        minute_of("2026-10-06T09:00:00Z", NOW),
        Ok("0 9 6 10 *".to_owned()),
        "a moment on the minute stays on it"
    );
}

/// The lead is inclusive: exactly a minute out is accepted, a millisecond
/// short of it is not, and a moment already gone is refused.
#[test]
fn a_moment_inside_the_lead_or_gone_is_refused() {
    let past = "at must be at least a minute in the future";
    assert_eq!(
        minute_of("2026-10-05T00:01:00Z", NOW),
        Ok("1 0 5 10 *".to_owned())
    );
    let just_after = NOW.saturating_add(ONE_MILLISECOND).expect("in range");
    assert_eq!(minute_of("2026-10-05T00:01:00Z", just_after), Err(past));
    assert_eq!(minute_of("2026-10-05T00:00:00Z", NOW), Err(past));
    assert_eq!(minute_of("2026-10-04T09:00:00Z", NOW), Err(past));
}

/// A cron has no year: the horizon's last minute is accepted, the next
/// refused, so an expression never fires a year early.
#[test]
fn the_horizon_is_a_year() {
    let too_far = "at must fall within the next year";
    let horizon = DAY.saturating_mul(365);
    let last = NOW.saturating_add(horizon).expect("in range");
    let past = (NOW.saturating_add(horizon + Duration::from_mins(1))).expect("in range");

    assert_eq!(
        minute_of(&last.to_string(), NOW),
        Ok("0 0 5 10 *".to_owned())
    );
    assert_eq!(minute_of(&past.to_string(), NOW), Err(too_far));
    assert_eq!(minute_of("2027-10-06T00:00:00Z", NOW), Err(too_far));
}

#[test]
fn an_unreadable_moment_is_refused() {
    let unreadable = "at must be an RFC 3339 instant such as 2026-10-06T09:00:00+05:30";
    for at in [
        "next tuesday",
        "",
        "2026-13-01T00:00:00Z",
        "２０２６-10-06T09:00:00Z",
    ] {
        assert_eq!(minute_of(at, NOW), Err(unreadable), "{at:?}");
    }
}

/// An expression whose next match lies past the last moment the calendar
/// holds never fires, and is refused as one that never matches.
#[test]
fn a_cron_whose_next_match_is_past_the_calendars_end_never_matches() {
    let near_the_end: Timestamp = "9999-12-30T00:00:00Z".parse().expect("a moment");

    let next = super::moment::next_after("0 0 1 1 *", "UTC", near_the_end);

    assert_eq!(
        next,
        Err(Refusal::new(
            code::INVALID_ARGUMENTS,
            "cron never matches a moment"
        ))
    );
}
