//! Whether a schedule will run, and when, as the server reads it at one instant.
//!
//! The dashboard used to compute the next run in the browser, from the
//! expression alone. That answered with a time for a schedule that was paused,
//! not yet registered, or registered nowhere, which is the misleading case the
//! server can rule out because it holds the status fields.

use afd_api_wire::schedule::RunState;
use jiff::Timestamp;
use philiprehberger_cron_parser::CronExpr;

use crate::model::{DesiredStatus, Schedule, SyncStatus};
use crate::next::next_fire;

/// `schedule`'s run state at `now`, with the next run when there is one.
///
/// Status decides first, because an expression's next match means nothing for
/// a schedule that will not fire: paused is paused whatever it is synced to,
/// a delete in flight never runs again, and only a registered schedule fires.
#[must_use]
pub(crate) fn upcoming(schedule: &Schedule, now: Timestamp) -> (RunState, Option<Timestamp>) {
    match (schedule.desired_status, schedule.sync_status) {
        (DesiredStatus::Paused, _) => (RunState::Paused, None),
        (DesiredStatus::Deleting, _) | (DesiredStatus::Active, SyncStatus::Failed) => {
            (RunState::NextRunUnavailable, None)
        }
        (DesiredStatus::Active, SyncStatus::Syncing) => (RunState::ActivationPending, None),
        (DesiredStatus::Active, SyncStatus::Synced) => next_match(schedule, now)
            .map_or((RunState::NextRunUnavailable, None), |at| {
                (RunState::Scheduled, Some(at))
            }),
    }
}

/// The expression in words, with its zone.
///
/// The words come from the cron parser the scheduler already uses, so the
/// description and the next run read one grammar. An expression the parser
/// refuses is shown as written: validation runs on every write, so a stored
/// expression that does not read is a defect to see, not one to hide.
#[must_use]
pub(crate) fn describe(cron: &str, timezone: &str) -> String {
    let words = CronExpr::parse(cron).map_or_else(
        |_| cron.to_owned(),
        |parsed| past_midnight(cron, parsed.describe()),
    );
    format!("{words} ({timezone})")
}

/// The parser's words, with the minute it drops in the midnight hour put back.
///
/// `philiprehberger-cron-parser` 0.3.0 hard-codes "12:00 AM" for hour 0
/// whatever the minute, so `30 0 * * *` would read as midnight. Every other hour
/// formats its minute; this restores the one it loses, and only when the
/// expression's minute field is a single value.
fn past_midnight(cron: &str, words: String) -> String {
    match cron.split_whitespace().next().map(str::parse::<u8>) {
        Some(Ok(minute)) if minute > 0 => {
            words.replacen("At 12:00 AM", &format!("At 12:{minute:02} AM"), 1)
        }
        _ => words,
    }
}

/// The next instant a registered schedule fires after `now`.
///
/// A one-off fires at the instant stored when its expression was set
/// ([`Schedule::fire_at`]), never at the expression's next match, which would
/// be a year out once the minute has passed. A one-off whose instant is behind
/// `now` has no next run.
fn next_match(schedule: &Schedule, now: Timestamp) -> Option<Timestamp> {
    if schedule.once {
        schedule.fire_at.filter(|at| *at > now)
    } else {
        next_fire(&schedule.cron, &schedule.timezone, now)
    }
}

#[cfg(test)]
mod tests;
