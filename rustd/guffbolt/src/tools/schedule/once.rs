//! `schedule`: one follow-up at a moment, kept as a job that retires after
//! its first fire.
//!
//! The moment becomes the five-field expression that matches only its minute
//! (minute, hour, day and month in UTC), added with `once`, so the scheduler
//! fires it and then retires it. A cron has no year, so the moment must fall
//! within the next year, where that expression matches exactly once.
//!
//! # Rounded up, and at least a minute out
//!
//! A cron fires on a minute's first second. A moment inside a minute rounds up
//! to the next one, so the run is never early, and the rounded minute must be
//! at least [`LEAD`] away: a scheduler has to hold the job before its minute
//! begins, or the expression's next match is a year later.

use std::time::Duration;

use jiff::tz::TimeZone;
use jiff::{RoundMode, Timestamp, TimestampRound, Unit};
use schemars::JsonSchema;
use serde::Deserialize;

use super::{Job, Scheduler, answered};
use crate::tool::ToolOutput;
use crate::tools::{Context, Handler, code};

/// The zone the expression is written in.
const UTC: &str = "UTC";
/// One minute, the resolution a cron fires at.
const MINUTE: Duration = Duration::from_mins(1);
/// How far ahead a moment may be: the span inside which its expression
/// matches only once.
const HORIZON: Duration = Duration::from_hours(24 * 365);
/// How far ahead the rounded minute must be, so the job is registered before
/// that minute begins.
const LEAD: Duration = MINUTE;
/// What an unreadable moment reads back.
const DETAIL_AT: &str = "at must be an RFC 3339 instant such as 2026-10-06T09:00:00+05:30";
/// What a moment already gone, or too close to register, reads back.
const DETAIL_PAST: &str = "at must be at least a minute in the future";
/// What a moment past the horizon reads back.
const DETAIL_TOO_FAR: &str = "at must fall within the next year";

/// `schedule`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct At {
    /// When to run again, as an RFC 3339 instant such as
    /// `2026-10-06T09:00:00+05:30`, within the next year. Read to the minute.
    at: String,
    /// What the run is asked to do then.
    message: String,
}

backed! {
    /// Schedules one follow-up run.
    ScheduleOnce over Scheduler
}

impl Handler for ScheduleOnce<'_> {
    const NAME: &'static str = "schedule";
    const DESCRIPTION: &'static str = "Schedule one more run at a given moment, with a message \
        saying what to do then. It is removed after it runs.";
    type Arguments = At;

    async fn run(&self, arguments: At, context: Context<'_>) -> ToolOutput {
        let cron = match minute_of(&arguments.at, Timestamp::now()) {
            Ok(cron) => cron,
            Err(detail) => return ToolOutput::failed(code::INVALID_ARGUMENTS, detail),
        };
        let job = Job {
            cron: &cron,
            timezone: Some(UTC),
            message: &arguments.message,
            once: true,
        };
        answered(self.backend.add(context.call_ref, job).await)
    }
}

/// The UTC expression that fires at `at`, rounded up to its minute, or why
/// it cannot, reading `now` as the present.
pub(super) fn minute_of(at: &str, now: Timestamp) -> Result<String, &'static str> {
    let instant: Timestamp = at.parse().map_err(|_unreadable| DETAIL_AT)?;
    let up_to_minute = TimestampRound::new()
        .smallest(Unit::Minute)
        .mode(RoundMode::Ceil);
    let minute = instant
        .round(up_to_minute)
        .map_err(|_range| DETAIL_TOO_FAR)?;
    if minute < after(now, LEAD) {
        return Err(DETAIL_PAST);
    }
    if minute > after(now, HORIZON) {
        return Err(DETAIL_TOO_FAR);
    }
    let civil = minute.to_zoned_datetime(TimeZone::UTC).datetime();
    Ok(format!(
        "{} {} {} {} *",
        civil.minute(),
        civil.hour(),
        civil.day(),
        civil.month()
    ))
}

/// `span` after `now`, saturating at the end of time: a `Duration` cannot
/// make jiff's arithmetic fail, so the fallback is the saturation point.
fn after(now: Timestamp, span: Duration) -> Timestamp {
    now.saturating_add(span).unwrap_or(Timestamp::MAX)
}
