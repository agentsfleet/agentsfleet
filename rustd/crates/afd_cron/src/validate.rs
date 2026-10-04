//! What this daemon will accept as a schedule, decided before anything is stored.
//!
//! # The parser is the crate's; the policy is this daemon's
//!
//! [`CronExpr::parse`] does the parsing — field splitting, bounds, ranges,
//! steps, lists — and this file adds nothing to it (RULE PSR). What it does add
//! is three refusals for expressions the crate ACCEPTS and the external
//! scheduler will not act on the way their author meant. They are the
//! differential cases the spec's Prior-Art table recorded:
//!
//! - **Aliases and names.** `@daily` and `MON` are read by the crate and are
//!   not what this daemon registers upstream. Refused here so the author sees
//!   it at create time, rather than storing an expression that fails when it is
//!   pushed.
//! - **A step wider than its field's span.** `*/61` in a minute field means
//!   "every 61st minute of 60", which is not a schedule.
//! - **A reversed range.** `5-2` reads as an empty set to one implementation
//!   and as `2-5` to another, so a schedule carrying one fires differently
//!   depending on who parses it.
//!
//! Those last two are the crate's bugs. The guard holds them until they are
//! fixed upstream, and each is a check ON TOP of a successful parse — none of
//! them re-implements one.
//!
//! # The bounds come first, and the type says so
//!
//! Each field is bounded before anything reads it: the parser's work and the
//! timezone lookup's filesystem read are both keyed on caller text. garde runs
//! every rule on a field with no short-circuit, so a parser declared beside a
//! bound would still be handed an oversized value. The bounds therefore live on
//! [`Fields`], and the three readers take `&garde::Valid<Fields>` — a value only
//! a passed validation constructs.

use afd_validate::PathTable;
use garde::{Unvalidated, Valid};
use jiff::tz::TimeZone;
use philiprehberger_cron_parser::CronExpr;

/// The longest expression this daemon will read.
///
/// A bound on the work one create can ask of the parser, and a value no
/// legitimate five-field expression comes near.
pub const MAX_CRON_LEN: usize = 128;

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

/// The span of each field, in the order an expression writes them.
///
/// Read only by the step guard, which needs to know what "wider than its own
/// field" means. The crate owns the bounds themselves and refuses a value
/// outside them; this is the one question it does not ask.
const FIELD_SPANS: [u16; 5] = [60, 24, 31, 12, 8];

/// The character an alias is introduced by.
const ALIAS_PREFIX: char = '@';

/// The characters a numeric field may carry beside digits.
const FIELD_PUNCTUATION: [char; 4] = ['*', ',', '-', '/'];

/// The character a field's alternatives are separated by.
const LIST_SEPARATOR: char = ',';

/// The character a range's ends are separated by.
const RANGE_SEPARATOR: char = '-';

/// The character a step is introduced by.
const STEP_SEPARATOR: char = '/';

/// Whether the expression, if the fields carry one, is one this daemon will
/// register.
///
/// # Errors
/// [`Invalid::Cron`] for an expression the parser refuses, and for the three it
/// accepts that this daemon does not — see the module note.
pub fn cron(fields: &Valid<Fields<'_>>) -> Result<(), Invalid> {
    let Some(expression) = fields.expression else {
        return Ok(());
    };

    // The parser first: everything it refuses is refused, and the guard below
    // only ever narrows what it accepted.
    CronExpr::parse(expression).map_err(|_refused| Invalid::Cron)?;

    if expression.trim_start().starts_with(ALIAS_PREFIX) {
        return Err(Invalid::Cron);
    }

    let admissible = expression
        .split_whitespace()
        .zip(FIELD_SPANS)
        .all(|(field, span)| {
            numeric_only(field)
                && field
                    .split(LIST_SEPARATOR)
                    .all(|item| step_within_span(item, span) && range_is_ordered(item))
        });
    if admissible {
        Ok(())
    } else {
        Err(Invalid::Cron)
    }
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

/// Whether a field carries only digits and the punctuation cron gives meaning.
///
/// The name guard. `MON` parses in the crate and is not what this daemon
/// registers; refusing it here is the difference between an author seeing the
/// problem at create time and a schedule that silently never fires.
fn numeric_only(field: &str) -> bool {
    field
        .chars()
        .all(|character| character.is_ascii_digit() || FIELD_PUNCTUATION.contains(&character))
}

/// Whether an item's step, if it has one, fits inside its field.
fn step_within_span(item: &str, span: u16) -> bool {
    let Some((_base, step)) = item.split_once(STEP_SEPARATOR) else {
        return true;
    };
    step.parse::<u16>()
        .is_ok_and(|step| step != 0 && step <= span)
}

/// Whether an item's range, if it has one, runs forwards.
fn range_is_ordered(item: &str) -> bool {
    let base = item
        .split_once(STEP_SEPARATOR)
        .map_or(item, |(base, _step)| base);
    let Some((start, end)) = base.split_once(RANGE_SEPARATOR) else {
        return true;
    };
    match (start.parse::<u16>(), end.parse::<u16>()) {
        (Ok(start), Ok(end)) => start <= end,
        // Ends the crate parsed and this does not are left to the crate's own
        // verdict rather than second-guessed here.
        _unparsed => true,
    }
}
