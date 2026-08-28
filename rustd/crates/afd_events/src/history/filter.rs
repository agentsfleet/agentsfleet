//! What narrows a page of history: an actor glob, and a lower bound on time.
//!
//! Both are the caller's, both reach SQL, and neither reaches it as text the
//! caller wrote. The glob is translated into a LIKE pattern with every
//! metacharacter escaped, and `since=` is resolved to an integer before it goes
//! anywhere near a statement.

use afd_core::clock::UnixMillis;
use jiff::Timestamp;

use crate::error::Error;

/// Milliseconds in each unit a `since=` duration may name.
const MS_PER_SECOND: i64 = 1_000;
const MS_PER_MINUTE: i64 = 60 * MS_PER_SECOND;
const MS_PER_HOUR: i64 = 60 * MS_PER_MINUTE;
const MS_PER_DAY: i64 = 24 * MS_PER_HOUR;

/// The exact length of the timestamp form `since=` accepts.
///
/// `YYYY-MM-DDTHH:MM:SSZ`. Checked before parsing so acceptance matches
/// `parseRfc3339Z` rather than matching whatever [`Timestamp`] is willing to
/// read — jiff alone would take an offset and a fractional second, which is a
/// WIDER surface than the daemon this ports and would have the two answering
/// differently during the migration.
const RFC3339_Z_LEN: usize = 20;

/// What narrows a listing, once every caller-supplied value is resolved.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filter {
    /// A SQL LIKE pattern, already escaped by [`glob_to_like`].
    pub actor_like: Option<String>,
    /// An inclusive lower bound on `created_at`.
    ///
    /// [`UnixMillis`] rather than a bare `i64`, because that is the type every
    /// timestamp in this workspace already is — `schema/`'s BIGINT columns,
    /// `afd_wire`'s fields and a `UUIDv7`'s own layout all agree on
    /// epoch-milliseconds, and `afd_core::clock` exists so the agreement has a
    /// name. An `i64` here would be a second spelling of it that nothing checks.
    pub since: Option<UnixMillis>,
}

/// Resolve a `since=` value against `now_ms`.
///
/// Two forms, exactly as `parseSince` takes them:
/// - a duration — `15s`, `30m`, `2h`, `7d` — meaning `now - duration`
/// - `YYYY-MM-DDTHH:MM:SSZ`, meaning that instant
///
/// `now` is a parameter rather than a clock read so a test is deterministic —
/// the seam `afd_core::clock` exists to provide.
///
/// # Why jiff appears here at all
///
/// `afd_core::clock` is the instant TYPE and the clock SOURCE; it carries no
/// calendar, and the absolute form of `since=` is a calendar string. So the
/// division is the one `afd_billing::window` already draws: [`UnixMillis`]
/// at both boundaries, [`Timestamp`] only for the civil-to-instant conversion
/// in between, and no epoch arithmetic hand-written anywhere.
///
/// # Errors
/// A window supplied ALONGSIDE a cursor is not refused here — that is the
/// caller's check,
/// because only the caller knows whether a cursor also arrived. Everything this
/// function refuses is an unreadable window value.
pub fn parse_since(input: &str, now: UnixMillis) -> Result<UnixMillis, Error> {
    let Some(last) = input.chars().last() else {
        return Err(Error::CursorMalformed);
    };
    let unit_ms = match last {
        's' => Some(MS_PER_SECOND),
        'm' => Some(MS_PER_MINUTE),
        'h' => Some(MS_PER_HOUR),
        'd' => Some(MS_PER_DAY),
        _timestamp_form => None,
    };
    match unit_ms {
        Some(unit) => parse_duration(&input[..input.len() - last.len_utf8()], unit, now),
        None => parse_rfc3339_z(input),
    }
}

/// `15s` and friends: the count before the unit, refusing a negative window.
///
/// Saturating rather than wrapping: a caller naming a duration wider than the
/// epoch gets the beginning of time, which is what they asked for, instead of
/// an arithmetic wrap that would silently become a window in the future.
fn parse_duration(digits: &str, unit_ms: i64, now: UnixMillis) -> Result<UnixMillis, Error> {
    let count: i64 = digits.parse().map_err(|_digits| Error::CursorMalformed)?;
    if count < 0 {
        return Err(Error::CursorMalformed);
    }
    Ok(UnixMillis::from_millis(
        now.as_millis()
            .saturating_sub(count.saturating_mul(unit_ms)),
    ))
}

/// The absolute form, shape-checked before it is parsed.
///
/// **Declared divergence.** `parseRfc3339Z` validates the day as `1..=31` for
/// every month and then runs a days-from-civil conversion, so it ACCEPTS
/// `2026-02-31T00:00:00Z` and silently rolls it into March. [`Timestamp`]
/// refuses it. That is a narrowing, which `docs/REST_API_DESIGN_GUIDELINES.md`
/// §9 classes as breaking — taken deliberately, because an impossible calendar
/// date is not a client contract and rolling it over answers a question nobody
/// asked. Recorded in the spec's Discovery rather than left for a reader to
/// discover from a diff.
fn parse_rfc3339_z(input: &str) -> Result<UnixMillis, Error> {
    if input.len() != RFC3339_Z_LEN || !input.ends_with('Z') {
        return Err(Error::CursorMalformed);
    }
    let at: Timestamp = input.parse().map_err(|_shape| Error::CursorMalformed)?;
    Ok(UnixMillis::from_millis(at.as_millisecond()))
}

/// Translate a client glob into a SQL LIKE pattern.
///
/// `*` becomes `%`; `%`, `_` and `\` are escaped so a literal one cannot become
/// a wildcard. The backslash is the one whose absence Postgres treats as an
/// ERROR rather than a wrong match — a pattern ending in a lone `\` is an
/// unterminated escape sequence (SQLSTATE 22025), which reached the Zig daemon
/// as a 500 on a filter a user is entitled to type.
#[must_use]
pub fn glob_to_like(glob: &str) -> String {
    let mut pattern = String::with_capacity(glob.len());
    for character in glob.chars() {
        match character {
            '*' => pattern.push('%'),
            '%' | '_' | '\\' => {
                pattern.push('\\');
                pattern.push(character);
            }
            literal => pattern.push(literal),
        }
    }
    pattern
}

/// Translate a client's actor PREFIX into a SQL LIKE pattern.
///
/// `actor_prefix=webhook:` selects the same rows `actor=webhook:*` does, and
/// the two are separate parameters because under prefix mode a literal `*`
/// matches a literal `*`. So this escapes the metacharacters and appends the
/// wildcard, where [`glob_to_like`] translates one.
///
/// **Declared divergence.** `prefixToLike` escapes `%` and `_` and not `\`,
/// which leaves a prefix ending in a lone backslash an unterminated escape
/// sequence — SQLSTATE 22025, reaching the caller as a 500 on a filter they
/// are entitled to type. [`glob_to_like`] already took the fix on its own
/// side; taking it here too is what keeps one rule rather than two.
#[must_use]
pub fn prefix_to_like(prefix: &str) -> String {
    let mut pattern = String::with_capacity(prefix.len() + 1);
    for character in prefix.chars() {
        if matches!(character, '%' | '_' | '\\') {
            pattern.push('\\');
        }
        pattern.push(character);
    }
    pattern.push('%');
    pattern
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]

    use super::*;

    /// A fixed instant, so every assertion below reads as arithmetic.
    const NOW_MS: i64 = 1_735_689_600_000;

    /// The four unit widths, spelled out HERE rather than imported from the
    /// module under test.
    ///
    /// Importing `MS_PER_SECOND` and its siblings would make these assertions
    /// tautological — the test would multiply by the same constant the parser
    /// divides by and agree with itself no matter what either said. An
    /// independent oracle has to carry its own numbers; naming them is what
    /// keeps that independence from reading as a bare literal.
    const ONE_SECOND: i64 = 1_000;
    const ONE_MINUTE: i64 = 60_000;
    const ONE_HOUR: i64 = 3_600_000;
    const ONE_DAY: i64 = 86_400_000;

    fn now() -> UnixMillis {
        UnixMillis::from_millis(NOW_MS)
    }

    /// The instant `raw` resolves to, in milliseconds.
    fn resolved(raw: &str) -> i64 {
        parse_since(raw, now())
            .expect("a window this test declares readable")
            .as_millis()
    }

    #[test]
    fn resolves_every_duration_unit() {
        assert_eq!(resolved("15s"), NOW_MS - 15 * ONE_SECOND);
        assert_eq!(resolved("30m"), NOW_MS - 30 * ONE_MINUTE);
        assert_eq!(resolved("2h"), NOW_MS - 2 * ONE_HOUR);
        assert_eq!(resolved("7d"), NOW_MS - 7 * ONE_DAY);
    }

    #[test]
    fn a_zero_duration_is_now_not_a_refusal() {
        assert_eq!(resolved("0s"), NOW_MS);
    }

    #[test]
    fn an_absurd_duration_saturates_instead_of_wrapping() {
        // A wrap would put the lower bound in the FUTURE and return nothing,
        // which reads to a caller as "this fleet has no history".
        let floor = resolved(&format!("{}d", i64::MAX));
        assert!(floor < NOW_MS, "a wider window must not land ahead of now");
    }

    #[test]
    fn reads_the_absolute_form() {
        assert_eq!(resolved("2025-01-01T00:00:00Z"), NOW_MS);
    }

    #[test]
    fn refuses_a_window_it_cannot_read() {
        for raw in [
            "",
            "s",                         // unit with no count
            "-5m",                       // negative window
            "abcd",                      // neither form
            "2025-01-01T00:00:00",       // no zone marker
            "2025-01-01T00:00:00+00:00", // an offset: wider than the Zig shape
            "2025-01-01T00:00:00.5Z",    // fractional: also wider
        ] {
            assert!(parse_since(raw, now()).is_err(), "accepted {raw:?}");
        }
    }

    #[test]
    fn refuses_an_impossible_calendar_date() {
        // The declared divergence, pinned: the Zig rolls this into March.
        parse_since("2026-02-31T00:00:00Z", now()).expect_err("the 31st of February is not a date");
    }

    #[test]
    fn translates_a_glob_to_a_like_pattern() {
        assert_eq!(glob_to_like("steer:*"), "steer:%");
        assert_eq!(glob_to_like("webhook:github"), "webhook:github");
    }

    #[test]
    fn a_prefix_becomes_a_pattern_the_glob_form_would_agree_with() {
        // The two parameters are meant to select the same rows for a prefix
        // carrying no metacharacter, and that agreement is the whole reason
        // both exist.
        assert_eq!(prefix_to_like("webhook:"), "webhook:%");
        assert_eq!(prefix_to_like("webhook:"), glob_to_like("webhook:*"));
        assert_eq!(prefix_to_like(""), "%");
    }

    #[test]
    fn a_star_in_a_prefix_stays_a_star() {
        // The difference between the two parameters: under prefix mode a
        // literal `*` matches a literal `*`, never a wildcard.
        assert_eq!(prefix_to_like("we_b%h*"), "we\\_b\\%h*%");
    }

    #[test]
    fn a_prefix_escapes_the_backslash_the_zig_left_bare() {
        // The declared divergence, pinned: a trailing lone backslash is an
        // unterminated escape sequence, and Postgres answers SQLSTATE 22025
        // rather than simply not matching.
        assert_eq!(prefix_to_like("path\\"), "path\\\\%");
    }

    #[test]
    fn escapes_every_metacharacter_a_caller_may_type() {
        assert_eq!(glob_to_like("100%"), "100\\%");
        assert_eq!(glob_to_like("a_b"), "a\\_b");
        // The one that was an ERROR rather than a wrong match: a trailing lone
        // backslash is SQLSTATE 22025.
        assert_eq!(glob_to_like("path\\"), "path\\\\");
        assert_eq!(glob_to_like("a\\b"), "a\\\\b");
    }
}
