//! A cron trigger's schedule: what this product will register, decided in one
//! place for the runner's proposal check, the daemon's definition parse and the
//! schedule routes alike.
//!
//! # The parser is the crate's; the policy is this product's
//!
//! [`CronExpr::parse`] does the parsing — field splitting, bounds, ranges,
//! steps, lists — and this file adds nothing to it (RULE PSR). What it does add
//! is three refusals for expressions the crate ACCEPTS and the external
//! scheduler will not act on the way their author meant:
//!
//! - **Aliases and names.** `@daily` and `MON` are read by the crate and are
//!   not what the daemon registers upstream. Refused here so the author sees
//!   it when the definition is checked, rather than storing an expression that
//!   fails when it is pushed.
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
//! # The bound comes first, and the type says so
//!
//! The parser's work is keyed on caller text, so [`check_schedule`] takes a
//! `&Valid<CronSchedule>`: a value only a passed [`MAX_CRON_LEN`] bound
//! constructs.

use garde::{Unvalidated, Valid};
use philiprehberger_cron_parser::CronExpr;

use super::{Cron, DEFAULT_CRON_MESSAGE, DEFAULT_CRON_TIMEZONE, REASON_NO_SCHEDULE};
use crate::error::{Error, ErrorKind, Result};

/// The longest expression this product will read.
///
/// A bound on the work one check can ask of the parser, and a value no
/// legitimate five-field expression comes near.
pub const MAX_CRON_LEN: usize = 128;

/// Why a cron trigger's schedule was refused. It names the key, so an author
/// or a model reading the refusal knows which value to change.
const REASON_INVALID_SCHEDULE: &str = "a cron trigger's `schedule` must be five numeric fields, \
     with no alias or name, each step inside its field and each range running forwards";

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

/// A cron expression as authored, with the bound it must hold before the
/// parser reads it.
#[derive(Debug, Clone, Copy, garde::Validate)]
pub struct CronSchedule<'a> {
    /// The expression a schedule fires on.
    #[garde(length(bytes, min = 1, max = MAX_CRON_LEN))]
    pub expression: &'a str,
}

/// Whether a proved expression is one this product will register.
///
/// The daemon's schedule routes and the definition parse both call this, so a
/// schedule the runner's proposal check accepts is one the daemon registers.
///
/// # Errors
/// [`Error::InvalidTriggerSet`] naming `schedule`, for an expression the parser
/// refuses and for the three it accepts that this product does not — see the
/// module note.
pub fn check_schedule(schedule: &Valid<CronSchedule<'_>>) -> Result<()> {
    let expression = schedule.expression;
    // The parser first: everything it refuses is refused, and the guard after
    // it only ever narrows what it accepted.
    let admissible = CronExpr::parse(expression).is_ok()
        && !expression.trim_start().starts_with(ALIAS_PREFIX)
        && expression
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
        Err(refused(REASON_INVALID_SCHEDULE))
    }
}

impl Cron {
    /// Builds a cron trigger from its authored keys, defaulting the zone and
    /// the message.
    ///
    /// # Errors
    /// [`Error::InvalidTriggerSet`] for an absent or empty schedule, one past
    /// [`MAX_CRON_LEN`], or one [`check_schedule`] refuses.
    pub(super) fn parse(
        schedule: Option<String>,
        timezone: Option<String>,
        message: Option<String>,
    ) -> Result<Self> {
        let schedule = schedule
            .filter(|value| !value.is_empty())
            .ok_or_else(|| refused(REASON_NO_SCHEDULE))?;
        let proved = Unvalidated::new(CronSchedule {
            expression: &schedule,
        })
        .validate()
        .map_err(|_too_long| refused(REASON_INVALID_SCHEDULE))?;
        check_schedule(&proved)?;
        Ok(Self {
            schedule: schedule.into(),
            timezone: timezone
                .unwrap_or_else(|| DEFAULT_CRON_TIMEZONE.to_owned())
                .into(),
            message: message
                .unwrap_or_else(|| DEFAULT_CRON_MESSAGE.to_owned())
                .into(),
        })
    }
}

/// The trigger-set refusal carrying `reason`.
fn refused(reason: &'static str) -> Error {
    ErrorKind::InvalidTriggerSet { reason }.into()
}

/// Whether a field carries only digits and the punctuation cron gives meaning.
///
/// The name guard. `MON` parses in the crate and is not what the daemon
/// registers; refusing it here is the difference between an author seeing the
/// problem at check time and a schedule that silently never fires.
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

#[cfg(test)]
mod tests;
