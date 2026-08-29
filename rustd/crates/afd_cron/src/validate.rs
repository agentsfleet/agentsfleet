//! What this daemon will accept as a schedule, decided before anything is stored.
//!
//! # Why the grammar is here rather than in a parser crate
//!
//! The spec's Prior-Art table settled on `philiprehberger-cron-parser` plus a
//! parity guard, on the evidence that the crate agrees with the Zig grammar on
//! 16 of 21 differential cases and disagrees on five: names and macros
//! (`@daily`, `MON`), a step wider than its own field's span (`*/61`), and a
//! reversed range (`5-2`). The guard's job would be to reject those five.
//!
//! Writing that guard means encoding field bounds, range ordering and
//! step-versus-span here anyway — which is the whole grammar, because there is
//! nothing else in it. The crate would then decide only the cases the guard
//! already decided, and the dependency would buy a second opinion this daemon
//! is contractually obliged to overrule. RULE PSR asks for a standard parser
//! where parsing is the hard part; here the hard part is the POLICY, and the
//! policy is "exactly what the Zig accepts" because a schedule that validates
//! on one daemon and not the other breaks a cutover.
//!
//! **This is a recorded deviation from the spec's crate verdict, not an
//! oversight.** `validate.zig` is the oracle and this is its port.
//!
//! # The bound on each field is the field's own, not a shared one
//!
//! Minute 0-59, hour 0-23, day 1-31, month 1-12, weekday 0-7 — seven being
//! Sunday a second time, which every cron implementation accepts and none
//! documents. A shared 0-59 bound would accept hour 40 and month 0, and the
//! external scheduler would take them and fire at times nobody asked for.

/// The longest expression this daemon will read.
///
/// `validate.zig`'s `MAX_CRON_LEN`. A bound on the work one create can ask of
/// the parser, and a value no legitimate five-field expression comes near.
pub const MAX_CRON_LEN: usize = 128;

/// The longest zone name this daemon will read.
pub const MAX_TIMEZONE_LEN: usize = 64;

/// The longest message a schedule may carry.
pub const MAX_MESSAGE_LEN: usize = 8192;

/// Why an input was refused.
///
/// Three variants rather than one, because a person fixing a schedule needs to
/// know WHICH field they got wrong — and the route renders each to its own
/// sentence. A single `Invalid` would make the caller guess.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Invalid {
    /// The expression is not five fields this daemon can read.
    Cron,
    /// The zone is not a name this daemon will pass upstream.
    Timezone,
    /// The message is absent, too long, or nothing but whitespace.
    Message,
}

/// The inclusive bounds of one cron field.
#[derive(Debug, Clone, Copy)]
struct Bounds {
    /// The lowest value the field accepts.
    min: u16,
    /// The highest.
    max: u16,
}

/// The five fields, in the order an expression writes them.
const FIELD_BOUNDS: [Bounds; 5] = [
    Bounds { min: 0, max: 59 },
    Bounds { min: 0, max: 23 },
    Bounds { min: 1, max: 31 },
    Bounds { min: 1, max: 12 },
    Bounds { min: 0, max: 7 },
];

/// The character a field's alternatives are separated by.
const LIST_SEPARATOR: char = ',';

/// The character a range's ends are separated by.
const RANGE_SEPARATOR: char = '-';

/// The character a step is introduced by.
const STEP_SEPARATOR: char = '/';

/// The field value meaning every value in bounds.
const WILDCARD: &str = "*";

/// The character a zone's region and city are separated by.
const ZONE_SEPARATOR: char = '/';

/// Whether `expression` is one this daemon will register.
///
/// # Errors
/// [`Invalid::Cron`] for an expression that is empty, over
/// [`MAX_CRON_LEN`], not exactly five fields, or carrying a field this
/// grammar does not accept.
pub fn cron(expression: &str) -> Result<(), Invalid> {
    if expression.is_empty() || expression.len() > MAX_CRON_LEN {
        return Err(Invalid::Cron);
    }

    let mut fields = expression.split_whitespace();
    for bounds in FIELD_BOUNDS {
        let field = fields.next().ok_or(Invalid::Cron)?;
        if !valid_field(field, bounds) {
            return Err(Invalid::Cron);
        }
    }

    // A sixth field is refused rather than ignored. Six-field expressions are a
    // real grammar elsewhere — with seconds in front — so accepting one by
    // dropping the extra would register a schedule that fires sixty times more
    // often than its author wrote.
    if fields.next().is_some() {
        return Err(Invalid::Cron);
    }
    Ok(())
}

/// Whether `value` is a zone name this daemon will pass upstream.
///
/// Shape only, deliberately: the set of real zone names belongs to the
/// timezone database and changes without this daemon being rebuilt, so
/// refusing an unknown-but-well-formed name here would reject a zone the
/// external scheduler accepts. What is refused is anything that could be read
/// as something other than a zone by whatever parses it next.
///
/// # Errors
/// [`Invalid::Timezone`] for an empty name, one over [`MAX_TIMEZONE_LEN`], or
/// one carrying a character or separator placement this daemon will not send.
pub fn timezone(value: &str) -> Result<(), Invalid> {
    if value.is_empty() || value.len() > MAX_TIMEZONE_LEN {
        return Err(Invalid::Timezone);
    }

    let last = value.len().saturating_sub(1);
    let mut previous_slash = false;
    for (index, character) in value.char_indices() {
        let slash = character == ZONE_SEPARATOR;
        // Leading, trailing and doubled separators are all refused: each of
        // them makes a name that a path-joining consumer reads differently
        // from the way it was written.
        if slash && (index == 0 || previous_slash || index == last) {
            return Err(Invalid::Timezone);
        }
        if !slash && !is_zone_character(character) {
            return Err(Invalid::Timezone);
        }
        previous_slash = slash;
    }
    Ok(())
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

/// Whether every alternative in one field is in bounds.
fn valid_field(field: &str, bounds: Bounds) -> bool {
    !field.is_empty()
        && field
            .split(LIST_SEPARATOR)
            .all(|item| valid_item(item, bounds))
}

/// Whether one alternative — a value, a range, or either with a step — is.
fn valid_item(item: &str, bounds: Bounds) -> bool {
    // At most one step. `*/2/3` is refused rather than read as its first two
    // parts, because a sender that wrote it meant something this grammar has
    // no answer for.
    let mut parts = item.split(STEP_SEPARATOR);
    let Some(base) = parts.next() else {
        return false;
    };
    let step = parts.next();
    if parts.next().is_some() {
        return false;
    }

    if let Some(step) = step {
        // A step wider than the field's own span is refused rather than
        // clamped. `*/61` in a minute field means "every 61st minute of 60",
        // which is not a schedule — and a parser that silently read it as
        // "once an hour" would register something its author never wrote.
        let span = bounds.max - bounds.min + 1;
        let Some(step) = number(step) else {
            return false;
        };
        if step == 0 || step > span {
            return false;
        }
    }

    if base == WILDCARD {
        return true;
    }

    let mut ends = base.split(RANGE_SEPARATOR);
    let Some(start) = ends.next().and_then(number) else {
        return false;
    };
    let Some(end) = ends.next() else {
        return in_bounds(start, bounds);
    };
    if ends.next().is_some() {
        return false;
    }

    // A reversed range is refused, not normalised. `5-2` reads as an empty set
    // to one implementation and as `2-5` to another, so a schedule carrying one
    // fires differently depending on who parses it — the exact class of drift
    // this port exists to close.
    number(end)
        .is_some_and(|end| in_bounds(start, bounds) && in_bounds(end, bounds) && start <= end)
}

/// One field component as a number, when it is one.
///
/// Rejects names and macros by construction: `MON` and `@daily` are not
/// digits, so they never reach a bounds check. That is the port's rule — the
/// external scheduler this daemon registers against takes numeric fields, and
/// accepting a name here would store an expression that fails upstream with no
/// way for the author to see why.
fn number(value: &str) -> Option<u16> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    value.parse().ok()
}

/// Whether a value falls inside its field's bounds.
const fn in_bounds(value: u16, bounds: Bounds) -> bool {
    value >= bounds.min && value <= bounds.max
}

/// Whether a character may appear in a zone name outside a separator.
const fn is_zone_character(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '+')
}
