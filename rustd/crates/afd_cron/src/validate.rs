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
/// Three variants rather than one, because a person fixing a schedule needs to
/// know WHICH field they got wrong — and the route renders each to its own
/// sentence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Invalid {
    /// The expression is not one this daemon will register.
    Cron,
    /// The zone is not a name this daemon will pass upstream.
    Timezone,
    /// The message is absent, too long, or nothing but whitespace.
    Message,
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

/// Whether `expression` is one this daemon will register.
///
/// # Errors
/// [`Invalid::Cron`] for an expression the parser refuses, and for the three it
/// accepts that this daemon does not — see the module note.
pub fn cron(expression: &str) -> Result<(), Invalid> {
    if expression.is_empty() || expression.len() > MAX_CRON_LEN {
        return Err(Invalid::Cron);
    }

    // The parser first: everything it refuses is refused, and the guard below
    // only ever narrows what it accepted.
    CronExpr::parse(expression).map_err(|_refused| Invalid::Cron)?;

    if expression.trim_start().starts_with(ALIAS_PREFIX) {
        return Err(Invalid::Cron);
    }

    let fields = expression.split_whitespace();
    for (field, span) in fields.zip(FIELD_SPANS) {
        if !numeric_only(field) {
            return Err(Invalid::Cron);
        }
        for item in field.split(LIST_SEPARATOR) {
            if !step_within_span(item, span) || !range_is_ordered(item) {
                return Err(Invalid::Cron);
            }
        }
    }
    Ok(())
}

/// Whether `value` names a zone the system timezone database knows.
///
/// Resolved rather than pattern-matched. A shape check accepts `Foo/Bar` — it
/// has the right characters and the right separator — and this daemon would
/// then store it, register it upstream, and learn it was wrong from a vendor
/// error nobody reads. `TimeZone::get` asks the database that actually defines
/// the answer.
///
/// The length bound stays in front of it, because the lookup is a filesystem
/// read keyed on the name and an unbounded one is an unbounded path.
///
/// # Errors
/// [`Invalid::Timezone`] for an empty name, one over [`MAX_TIMEZONE_LEN`], or
/// one the timezone database does not define.
pub fn timezone(value: &str) -> Result<(), Invalid> {
    if value.is_empty() || value.len() > MAX_TIMEZONE_LEN {
        return Err(Invalid::Timezone);
    }
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

/// Whether `value` is a message worth waking a fleet with.
///
/// # Errors
/// [`Invalid::Message`] for an empty message, one over [`MAX_MESSAGE_LEN`], or
/// one that is nothing but whitespace — the last because a fleet woken with
/// nothing to do spends a model to decide it has nothing to do.
pub fn message(value: &str) -> Result<(), Invalid> {
    if value.is_empty() || value.len() > MAX_MESSAGE_LEN {
        return Err(Invalid::Message);
    }
    if value.chars().all(char::is_whitespace) {
        return Err(Invalid::Message);
    }
    Ok(())
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
