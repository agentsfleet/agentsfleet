//! What this daemon will accept as a schedule, decided before anything is stored.
//!
//! # The expression check is the fleet runtime's
//!
//! [`cron`] delegates to [`af_fleet_runtime::config::check_schedule`], the check
//! a fleet definition's cron trigger is parsed with. A schedule route and a
//! proposed definition therefore cannot disagree on what a schedule may say:
//! the parser, the alias and name refusals, the step guard and the range guard
//! all live there, once, and its module note says why each exists.
//!
//! # The bounds come first, and the type says so
//!
//! Each field is bounded before anything reads it: the parser's work and the
//! timezone lookup's filesystem read are both keyed on caller text. garde runs
//! every rule on a field with no short-circuit, so a parser declared beside a
//! bound would still be handed an oversized value. The bounds therefore live on
//! [`Fields`], and the three readers take `&garde::Valid<Fields>` — a value only
//! a passed validation constructs.

// The fleet runtime's bound, so a schedule route and a definition's cron
// trigger stop at the same length.
use af_fleet_runtime::config::{CronSchedule, MAX_CRON_LEN, check_schedule};
use af_validate::PathTable;
use const_format::concatcp;
use garde::{Unvalidated, Valid};
use jiff::tz::TimeZone;

/// The longest zone name this daemon will read.
pub const MAX_TIMEZONE_LEN: usize = 64;

/// The longest message a schedule may carry.
pub const MAX_MESSAGE_LEN: usize = 8192;

/// Why an input was refused.
///
/// One variant per repair, because a person fixing a schedule needs to know
/// WHICH field they got wrong and what to do about it — and the route renders
/// each to its own sentence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Invalid {
    /// The expression is not one this daemon will register.
    Cron,
    /// The zone is not a name this daemon will pass upstream.
    Timezone,
    /// The message is absent or nothing but whitespace.
    Message,
    /// The message is longer than [`MAX_MESSAGE_LEN`].
    MessageTooLong,
}

/// The sentence an expression this daemon will not register earns.
pub const DETAIL_INVALID_CRON: &str =
    "The cron expression must be five numeric fields this daemon accepts.";

/// The sentence a zone this daemon will not pass upstream earns.
pub const DETAIL_INVALID_TIMEZONE: &str = "The timezone is not a name this daemon will register.";

/// The sentence a message that would wake a fleet with nothing earns.
pub const DETAIL_INVALID_MESSAGE: &str = "The message must not be empty.";

/// The sentence a message past [`MAX_MESSAGE_LEN`] earns.
///
/// Its own sentence: an oversized message is not an empty one, and telling a
/// caller who sent eight kilobytes that they sent nothing sends them to fix
/// the wrong thing.
pub const DETAIL_MESSAGE_TOO_LONG: &str =
    concatcp!("The message must be at most ", MAX_MESSAGE_LEN, " bytes.");

impl Invalid {
    /// The sentence a caller is told: one per repair, so a person and a fleet
    /// fixing a schedule read the same words from either surface.
    #[must_use]
    pub const fn detail(self) -> &'static str {
        match self {
            Self::Cron => DETAIL_INVALID_CRON,
            Self::Timezone => DETAIL_INVALID_TIMEZONE,
            Self::Message => DETAIL_INVALID_MESSAGE,
            Self::MessageTooLong => DETAIL_MESSAGE_TOO_LONG,
        }
    }
}

/// A schedule's three authored fields, with the bound each must hold.
///
/// Every field is optional because a patch names only what it changes; an
/// absent field is neither bounded nor read.
#[derive(Debug, Clone, Copy, Default, garde::Validate)]
pub struct Fields<'a> {
    /// The expression it fires on.
    #[garde(length(bytes, min = 1, max = MAX_CRON_LEN))]
    pub expression: Option<&'a str>,
    /// The zone that expression is read in.
    #[garde(length(bytes, min = 1, max = MAX_TIMEZONE_LEN))]
    pub timezone: Option<&'a str>,
    /// What the fleet is asked to do. Only the cap is a bound: an empty or
    /// blank message is [`message`]'s refusal, a different repair.
    #[garde(length(bytes, max = MAX_MESSAGE_LEN))]
    pub message: Option<&'a str>,
}

/// The path garde reports an expression break under.
const PATH_EXPRESSION: &str = "expression";
/// The path garde reports a zone break under.
const PATH_TIMEZONE: &str = "timezone";
/// The path garde reports a message break under.
const PATH_MESSAGE: &str = "message";

/// The refusal each bound answers, in the order the fields are read.
const BOUNDS: PathTable<Invalid> = PathTable::new(
    &[
        (PATH_EXPRESSION, Invalid::Cron),
        (PATH_TIMEZONE, Invalid::Timezone),
        (PATH_MESSAGE, Invalid::MessageTooLong),
    ],
    Invalid::Cron,
);

impl Fields<'_> {
    /// Proves every bound, then reads each field the schedule carries.
    ///
    /// # Errors
    /// The [`Invalid`] of the first field, in declaration order, that breaks
    /// its bound or that its reader refuses.
    pub fn check(self) -> Result<(), Invalid> {
        let proved = Unvalidated::new(self)
            .validate()
            .map_err(|report| BOUNDS.pick(&report))?;
        cron(&proved)?;
        timezone(&proved)?;
        message(&proved)
    }
}

/// Whether the expression, if the fields carry one, is one this daemon will
/// register.
///
/// # Errors
/// [`Invalid::Cron`] for every expression
/// [`af_fleet_runtime::config::check_schedule`] refuses: the parser's refusals
/// and the three it accepts that this daemon does not register.
pub fn cron(fields: &Valid<Fields<'_>>) -> Result<(), Invalid> {
    let Some(expression) = fields.expression else {
        return Ok(());
    };
    // `Fields` already held this bound, which is the fleet runtime's own; the
    // check takes its proof as a type, so it is shown once more here.
    let proved = Unvalidated::new(CronSchedule { expression })
        .validate()
        .map_err(|_too_long| Invalid::Cron)?;
    check_schedule(&proved).map_err(|_refused| Invalid::Cron)
}

/// Whether the zone, if the fields carry one, names a zone the system
/// timezone database knows.
///
/// Resolved rather than pattern-matched. A shape check accepts `Foo/Bar` — it
/// has the right characters and the right separator — and this daemon would
/// then store it, register it upstream, and learn it was wrong from a vendor
/// error nobody reads. `TimeZone::get` asks the database that actually defines
/// the answer. The lookup is a filesystem read keyed on the name, which is why
/// it takes a proved value: [`MAX_TIMEZONE_LEN`] held before it runs.
///
/// # Errors
/// [`Invalid::Timezone`] for a name carrying a traversal or one the timezone
/// database does not define.
pub fn timezone(fields: &Valid<Fields<'_>>) -> Result<(), Invalid> {
    let Some(value) = fields.timezone else {
        return Ok(());
    };
    // A name carrying a separator the database would resolve through the
    // filesystem is refused before the lookup: `..` in a zone name is a path
    // traversal into whatever else that directory holds.
    if value.contains("..") {
        return Err(Invalid::Timezone);
    }
    TimeZone::get(value)
        .map(|_zone| ())
        .map_err(|_unknown| Invalid::Timezone)
}

/// Whether the message, if the fields carry one, is worth waking a fleet with.
///
/// # Errors
/// [`Invalid::Message`] for an empty message or one that is nothing but
/// whitespace — a fleet woken with nothing to do spends a model to decide it
/// has nothing to do.
pub fn message(fields: &Valid<Fields<'_>>) -> Result<(), Invalid> {
    match fields.message {
        Some(value) if value.chars().all(char::is_whitespace) => Err(Invalid::Message),
        _absent_or_worded => Ok(()),
    }
}
