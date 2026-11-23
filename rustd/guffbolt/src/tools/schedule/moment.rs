//! When a job's expression next matches, read in its zone: the one piece of
//! calendar arithmetic the local scheduler does.

use jiff::Timestamp;
use jiff::tz::TimeZone;
use philiprehberger_cron_parser::{CronExpr, DateTime as CronTime};

use crate::tools::{Refusal, code};

/// What an expression the grammar refuses reads back, before the reason.
const BAD_CRON: &str = "cron must be five fields: minute, hour, day of month, month, day of week";
/// What an expression that never fires reads back.
const NEVER: &str = "cron never matches a moment";
/// What a zone the database lacks reads back, before the name.
const BAD_ZONE: &str = "timezone is not an IANA zone:";

/// The first moment after `from` that `cron` matches, read in `timezone`, or
/// why it has none.
pub(super) fn next_after(
    cron: &str,
    timezone: &str,
    from: Timestamp,
) -> Result<Timestamp, Refusal> {
    let invalid = |detail: String| Refusal::new(code::INVALID_ARGUMENTS, detail);
    let expression =
        CronExpr::parse(cron).map_err(|refused| invalid(format!("{BAD_CRON}: {refused}")))?;
    let zone =
        TimeZone::get(timezone).map_err(|_unknown| invalid(format!("{BAD_ZONE} {timezone}")))?;
    let civil = from.to_zoned_datetime(zone.clone()).datetime();
    let after = CronTime {
        year: i32::from(civil.year()),
        month: u8::try_from(civil.month()).unwrap_or_default(),
        day: u8::try_from(civil.day()).unwrap_or_default(),
        hour: u8::try_from(civil.hour()).unwrap_or_default(),
        minute: u8::try_from(civil.minute()).unwrap_or_default(),
        second: u8::try_from(civil.second()).unwrap_or_default(),
    };
    let next = expression
        .next_from(&after)
        .ok_or_else(|| invalid(NEVER.to_owned()))?;
    let year = i16::try_from(next.year).map_err(|_far| invalid(NEVER.to_owned()))?;
    let fields = [next.month, next.day, next.hour, next.minute]
        .map(|field| i8::try_from(field).unwrap_or_default());
    let [month, day, hour, minute] = fields;
    jiff::civil::DateTime::new(year, month, day, hour, minute, 0, 0)
        .and_then(|moment| moment.to_zoned_datetime(zone))
        .map(|zoned| zoned.timestamp())
        .map_err(|_unrepresentable| invalid(NEVER.to_owned()))
}
