//! The `custom` rules garde does not ship, each a function garde calls as
//! `rule(&field, &context)`.
//!
//! Every rule is generic over the context, so one spelling serves a struct
//! validated with `()` and one validated against a route's [`crate::Ceiling`].
//! A rule's message is for logs and tests; the sentence a caller reads comes
//! from the route's [`crate::Sentences`], never from here.
//!
//! Each rule judges content only. Emptiness and length are garde's own
//! `length`, declared beside the rule on the same field, so a rule never
//! answers for a bound it does not own.

/// What [`finite`] reports.
pub const NOT_FINITE: &str = "must be a finite number";
/// What [`nul_free`] reports.
pub const HAS_NUL: &str = "must not contain a NUL character";
/// What [`ascii_digits`] reports.
pub const NOT_ASCII_DIGITS: &str = "must be ASCII digits only";
/// What [`charset`] reports.
pub const OUTSIDE_CHARSET: &str = "must use only the characters this field allows";

/// Refuses NaN and both infinities.
///
/// garde's `range` compares with `<` and `>`, and every comparison against NaN
/// is false, so `range(min = 0.0)` admits NaN. Declare this beside the range on
/// every float a caller or an author supplies.
///
/// # Errors
/// [`NOT_FINITE`] for NaN, +∞ or −∞.
pub fn finite<C: ?Sized>(value: &f64, _context: &C) -> garde::Result {
    refuse_unless(value.is_finite(), NOT_FINITE)
}

/// Refuses a NUL character.
///
/// Postgres refuses NUL inside `text`, so a value that carries one fails at
/// the store as an internal error instead of at the boundary as the caller's.
///
/// # Errors
/// [`HAS_NUL`] when the text contains `\0`.
pub fn nul_free<C: ?Sized>(value: &str, _context: &C) -> garde::Result {
    refuse_unless(!value.contains('\0'), HAS_NUL)
}

/// Refuses anything but `0`–`9`.
///
/// Stricter than `str::parse::<u32>`, which takes a leading `+`: a parameter
/// documented as a number is digits, and a sign is a different request.
///
/// # Errors
/// [`NOT_ASCII_DIGITS`] when any character is not an ASCII digit.
pub fn ascii_digits<C: ?Sized>(value: &str, _context: &C) -> garde::Result {
    refuse_unless(
        value.bytes().all(|byte| byte.is_ascii_digit()),
        NOT_ASCII_DIGITS,
    )
}

/// A rule admitting only characters `allowed` accepts.
///
/// Called in the attribute — `#[garde(custom(charset(is_slug_char)))]` — so
/// the predicate is named beside the field it guards rather than hidden inside
/// a one-off rule function.
///
/// # Errors
/// The returned rule answers [`OUTSIDE_CHARSET`] when any character fails
/// `allowed`.
pub fn charset<C: ?Sized>(allowed: impl Fn(char) -> bool) -> impl Fn(&str, &C) -> garde::Result {
    move |value, _context| refuse_unless(value.chars().all(&allowed), OUTSIDE_CHARSET)
}

/// `Ok` when `holds`, otherwise a report carrying `message`.
fn refuse_unless(holds: bool, message: &'static str) -> garde::Result {
    if holds {
        Ok(())
    } else {
        Err(garde::Error::new(message))
    }
}

#[cfg(test)]
#[path = "rules/tests.rs"]
mod tests;
