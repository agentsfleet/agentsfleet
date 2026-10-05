//! `schedule`: one follow-up at a moment, kept as a schedule that retires
//! after its first fire.
//!
//! The moment becomes the five-field expression that matches only its minute
//! — minute, hour, day and month in UTC — created with `once`, so
//! `agentsfleetd` fires it through `QStash` and then retires it. A cron has no
//! year, so the moment must fall within the next year, where that expression
//! matches exactly once.
//!
//! # Rounded up, and at least a minute out
//!
//! A cron fires on a minute's first second. A moment inside a minute rounds up
//! to the next one, so the run is never early, and the rounded minute must be
//! at least [`LEAD_MS`] away: `QStash` has to hold the schedule before its
//! minute begins, or the expression's next match is a year later.

use afd_core::clock::{self, UnixMillis};
use afd_core::timing::DAY_MS;
use jiff::Timestamp;
use jiff::tz::TimeZone;
use schemars::JsonSchema;
use serde::Deserialize;

use super::{ScheduleCall, answered};
use crate::catalog::{Entry, SCHEDULE};
use crate::egress;
use crate::handler::Handler;
use crate::runtime::{ToolContext, ToolErrorCode, ToolOutput};

/// The zone the expression is written in.
const UTC: &str = "UTC";

/// How far ahead a moment may be: the span inside which its expression
/// matches only once.
const HORIZON_MS: i64 = 365 * DAY_MS;

/// One minute, the resolution a cron fires at.
const MINUTE_MS: i64 = 60_000;

/// How far ahead the rounded minute must be, so the schedule is registered
/// before that minute begins.
const LEAD_MS: i64 = MINUTE_MS;

/// What an unreadable moment reads back.
const DETAIL_AT: &str = "at must be an RFC 3339 instant such as 2026-10-06T09:00:00+05:30";

/// What a moment already gone, or too close to register, reads back.
const DETAIL_PAST: &str = "at must be at least a minute in the future";

/// What a moment past the horizon reads back.
const DETAIL_TOO_FAR: &str = "at must fall within the next year";

/// `schedule`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct At {
    /// When this fleet runs again, as an RFC 3339 instant such as
    /// `2026-10-06T09:00:00+05:30`, within the next year. Read to the minute.
    at: String,
    /// What this fleet is asked to do then.
    message: String,
}

/// Schedules one follow-up run of this fleet.
#[derive(Debug)]
pub(crate) struct ScheduleOnce;

#[async_trait::async_trait]
impl Handler for ScheduleOnce {
    const ENTRY: &'static Entry = &SCHEDULE;
    const DESCRIPTION: &'static str = "Schedule this fleet to run once more at a given moment, \
        with a message saying what to do then. It is removed after it runs.";
    type Arguments = At;

    async fn run(&self, arguments: At, context: ToolContext<'_, '_>) -> ToolOutput {
        let cron = match minute_of(&arguments.at, clock::now()) {
            Ok(cron) => cron,
            Err(detail) => return ToolOutput::failed(ToolErrorCode::InvalidArguments, detail),
        };
        let message = egress::masked(context.lease, arguments.message);
        let call = ScheduleCall::Create {
            cron: &cron,
            timezone: Some(UTC),
            message: &message,
            once: true,
        };
        answered(context.lease.verbs.schedules(call).await)
    }
}

/// The UTC expression that fires at `at`, rounded up to its minute, or why
/// it cannot.
fn minute_of(at: &str, now: UnixMillis) -> Result<String, &'static str> {
    let instant: Timestamp = at.parse().map_err(|_unreadable| DETAIL_AT)?;
    let millis = instant.as_millisecond();
    let to_next_minute = (MINUTE_MS - millis.rem_euclid(MINUTE_MS)) % MINUTE_MS;
    let minute = millis
        .checked_add(to_next_minute)
        .and_then(|rounded| Timestamp::from_millisecond(rounded).ok())
        .ok_or(DETAIL_TOO_FAR)?;
    let ahead = minute.as_millisecond() - now.as_millis();
    if ahead < LEAD_MS {
        return Err(DETAIL_PAST);
    }
    if ahead > HORIZON_MS {
        return Err(DETAIL_TOO_FAR);
    }
    let civil = minute.to_zoned(TimeZone::UTC).datetime();
    Ok(format!(
        "{} {} {} {} *",
        civil.minute(),
        civil.hour(),
        civil.day(),
        civil.month()
    ))
}

#[cfg(test)]
#[path = "once/tests.rs"]
mod tests;
