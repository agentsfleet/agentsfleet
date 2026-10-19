//! When a schedule's expression next matches, read in its own zone.
//!
//! A `once` schedule stores this instant at the moment its expression is set
//! ([`crate::model::Schedule::fire_at`]): the expression carries no year, so a
//! sync that registers it after the minute has passed would wait a year for the
//! next match. The reconciler retires such a schedule instead.

use jiff::Timestamp;
use jiff::civil;
use jiff::tz::TimeZone;
use philiprehberger_cron_parser::{CronExpr, DateTime};

/// The first instant after `after` that `cron` matches in `timezone`.
///
/// `None` for an expression or a zone that does not read, or a wall-clock
/// match the zone skips: nothing to store, so the schedule keeps the reading a
/// recurring one has.
pub(crate) fn next_fire(cron: &str, timezone: &str, after: Timestamp) -> Option<Timestamp> {
    let expression = CronExpr::parse(cron).ok()?;
    let zone = TimeZone::get(timezone).ok()?;
    let wall = after.to_zoned(zone.clone()).datetime();
    let next = expression.next_from(&DateTime {
        year: i32::from(wall.year()),
        month: u8::try_from(wall.month()).ok()?,
        day: u8::try_from(wall.day()).ok()?,
        hour: u8::try_from(wall.hour()).ok()?,
        minute: u8::try_from(wall.minute()).ok()?,
        second: u8::try_from(wall.second()).ok()?,
    })?;
    let civil = civil::DateTime::new(
        i16::try_from(next.year).ok()?,
        i8::try_from(next.month).ok()?,
        i8::try_from(next.day).ok()?,
        i8::try_from(next.hour).ok()?,
        i8::try_from(next.minute).ok()?,
        0,
        0,
    )
    .ok()?;
    Some(civil.to_zoned(zone).ok()?.timestamp())
}

#[cfg(test)]
mod tests;
