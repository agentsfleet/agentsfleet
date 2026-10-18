//! What narrows a page of history: an actor glob, and a lower bound on time.
//!
//! Both are the caller's, both reach SQL, and neither reaches it as text the
//! caller wrote. The glob is translated into a LIKE pattern with every
//! metacharacter escaped, and `since=` is resolved to a [`Since`] before it
//! goes anywhere near a statement.
//!
//! # The database reads the clock
//!
//! A relative window — `30m` — is a SPAN, and the statement subtracts it from
//! its own `statement_timestamp()`, the clock that stamped every row it
//! compares against. Nothing here reads a clock, so a parse is deterministic
//! without a seam for one.

use std::time::Duration;

use afd_core::timing::DAY;
use afd_db::statement::{LIKE_ANY, like_escape};
use jiff::Timestamp;

use crate::error::{Result, cursor_malformed};

/// The widest window a relative `since=` resolves to: a thousand years.
///
/// Wider than any history this product holds, and still a span Postgres can
/// subtract from now. A wider ask saturates here instead of overflowing.
pub const WIDEST_WINDOW: Duration = DAY.saturating_mul(365_000);

/// The exact length of the timestamp form `since=` accepts.
///
/// `YYYY-MM-DDTHH:MM:SSZ`. Checked before parsing so acceptance stays the
/// narrow `Z` shape rather than whatever [`Timestamp`] is willing to read —
/// jiff alone would take an offset and a fractional second.
const RFC3339_Z_LEN: usize = 20;

/// A lower bound on `created_at`, as the caller asked for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Since {
    /// The last `span`, measured back from the statement's own clock.
    Within(Duration),
    /// From a fixed instant, inclusive.
    From(Timestamp),
}

impl Since {
    /// The instant a listing binds: set for [`Since::From`], else NULL.
    #[must_use]
    pub const fn instant(window: Option<Self>) -> Option<Timestamp> {
        match window {
            Some(Self::From(at)) => Some(at),
            Some(Self::Within(_)) | None => None,
        }
    }

    /// The span a listing binds: set for [`Since::Within`], else NULL.
    #[must_use]
    pub const fn span(window: Option<Self>) -> Option<Duration> {
        match window {
            Some(Self::Within(span)) => Some(span),
            Some(Self::From(_)) | None => None,
        }
    }
}

/// What narrows a listing, once every caller-supplied value is resolved.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filter {
    /// A SQL LIKE pattern, already escaped by [`glob_to_like`].
    pub actor_like: Option<String>,
    /// An inclusive lower bound on `created_at`.
    pub since: Option<Since>,
}

/// Resolve a `since=` value.
///
/// Two forms:
/// - a duration — `15s`, `30m`, `2h`, `7d` — meaning that span back from now,
///   where now is the database's
/// - `YYYY-MM-DDTHH:MM:SSZ`, meaning that instant
///
/// # Errors
/// A window supplied ALONGSIDE a cursor is not refused here — that is the
/// caller's check, because only the caller knows whether a cursor also
/// arrived. Everything this function refuses is an unreadable window value.
pub fn parse_since(input: &str) -> Result<Since> {
    let Some(last) = input.chars().last() else {
        return Err(cursor_malformed());
    };
    let unit = match last {
        's' => Some(Duration::from_secs(1)),
        'm' => Some(Duration::from_mins(1)),
        'h' => Some(Duration::from_hours(1)),
        'd' => Some(DAY),
        _timestamp_form => None,
    };
    match unit {
        Some(unit) => parse_duration(&input[..input.len() - last.len_utf8()], unit),
        None => parse_rfc3339_z(input),
    }
}

/// `15s` and friends: the count before the unit, refusing a negative window.
///
/// Saturating at [`WIDEST_WINDOW`] rather than wrapping: a caller naming a
/// duration wider than any history gets all of it, which is what they asked
/// for, instead of an overflow.
fn parse_duration(digits: &str, unit: Duration) -> Result<Since> {
    let count: i64 = digits.parse().map_err(|_digits| cursor_malformed())?;
    if count < 0 {
        return Err(cursor_malformed());
    }
    let span = u32::try_from(count)
        .ok()
        .and_then(|count| unit.checked_mul(count))
        .unwrap_or(WIDEST_WINDOW);
    Ok(Since::Within(span.min(WIDEST_WINDOW)))
}

/// The absolute form, shape-checked before it is parsed.
///
/// An impossible calendar date — `2026-02-31T00:00:00Z` — is refused rather
/// than rolled into March: it is not a date, and rolling it over answers a
/// question nobody asked.
fn parse_rfc3339_z(input: &str) -> Result<Since> {
    if input.len() != RFC3339_Z_LEN || !input.ends_with('Z') {
        return Err(cursor_malformed());
    }
    let at: Timestamp = input.parse().map_err(|_shape| cursor_malformed())?;
    Ok(Since::From(at))
}

/// The glob wildcard a client types for any run of characters.
const GLOB_ANY: char = '*';

/// Translate a client glob into a SQL LIKE pattern.
///
/// `*` becomes `%`; `%`, `_` and `\` are escaped so a literal one cannot become
/// a wildcard. The backslash is the one whose absence Postgres treats as an
/// ERROR rather than a wrong match — a pattern ending in a lone `\` is an
/// unterminated escape sequence (SQLSTATE 22025), which would reach the caller
/// as a 500 on a filter a user is entitled to type.
#[must_use]
pub fn glob_to_like(glob: &str) -> String {
    glob.split(GLOB_ANY)
        .map(like_escape)
        .collect::<Vec<_>>()
        .join(&LIKE_ANY.to_string())
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
    let mut pattern = like_escape(prefix);
    pattern.push(LIKE_ANY);
    pattern
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]

    use super::*;

    /// The four unit widths, spelled out HERE rather than imported from the
    /// module under test, so the assertions are an independent oracle rather
    /// than the parser agreeing with itself.
    const ONE_SECOND: Duration = Duration::from_secs(1);
    const ONE_MINUTE: Duration = Duration::from_mins(1);
    const ONE_HOUR: Duration = Duration::from_hours(1);
    const ONE_DAY: Duration = Duration::from_hours(24);

    /// The window `raw` resolves to.
    fn resolved(raw: &str) -> Since {
        parse_since(raw).expect("a window this test declares readable")
    }

    #[test]
    fn resolves_every_duration_unit_to_a_span() {
        assert_eq!(resolved("15s"), Since::Within(ONE_SECOND * 15));
        assert_eq!(resolved("30m"), Since::Within(ONE_MINUTE * 30));
        assert_eq!(resolved("2h"), Since::Within(ONE_HOUR * 2));
        assert_eq!(resolved("7d"), Since::Within(ONE_DAY * 7));
    }

    #[test]
    fn a_zero_duration_is_now_not_a_refusal() {
        assert_eq!(resolved("0s"), Since::Within(Duration::ZERO));
    }

    #[test]
    fn an_absurd_duration_saturates_instead_of_wrapping() {
        // A wrap would put the lower bound in the FUTURE and return nothing,
        // which reads to a caller as "this fleet has no history".
        assert_eq!(
            resolved(&format!("{}d", i64::MAX)),
            Since::Within(WIDEST_WINDOW)
        );
        assert_eq!(
            resolved(&format!("{}d", u32::MAX)),
            Since::Within(WIDEST_WINDOW)
        );
    }

    #[test]
    fn reads_the_absolute_form_as_an_instant() {
        assert_eq!(
            resolved("2025-01-01T00:00:00Z"),
            Since::From(Timestamp::constant(1_735_689_600, 0))
        );
    }

    #[test]
    fn a_window_binds_exactly_one_of_its_two_slots() {
        let span = Some(Since::Within(ONE_HOUR));
        let at = Some(Since::From(Timestamp::UNIX_EPOCH));
        assert_eq!(
            (Since::instant(span), Since::span(span)),
            (None, Some(ONE_HOUR))
        );
        assert_eq!(
            (Since::instant(at), Since::span(at)),
            (Some(Timestamp::UNIX_EPOCH), None)
        );
        assert_eq!((Since::instant(None), Since::span(None)), (None, None));
    }

    #[test]
    fn refuses_a_window_it_cannot_read() {
        for raw in [
            "",
            "s",                         // unit with no count
            "-5m",                       // negative window
            "abcd",                      // neither form
            "2025-01-01T00:00:00",       // no zone marker
            "2025-01-01T00:00:00+00:00", // an offset: wider than the `Z` shape
            "2025-01-01T00:00:00.5Z",    // fractional: also wider
        ] {
            assert!(parse_since(raw).is_err(), "accepted {raw:?}");
        }
    }

    #[test]
    fn refuses_an_impossible_calendar_date() {
        // Pinned: a parser that normalises the date would roll this into March.
        parse_since("2026-02-31T00:00:00Z").expect_err("the 31st of February is not a date");
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
    fn a_prefix_escapes_a_trailing_backslash() {
        // Pinned: a trailing lone backslash is an unterminated escape
        // sequence, and Postgres answers SQLSTATE 22025 rather than simply not
        // matching.
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
