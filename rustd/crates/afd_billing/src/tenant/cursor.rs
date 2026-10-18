//! The charges endpoint's opaque cursor: base64url of `{created_at}|{id}`,
//! the instant written as RFC 3339 in UTC.
//!
//! # A second cursor format, not a variant of the first
//!
//! The tenant plane already has a keyset cursor — [`afd_core::paging::Cursor`],
//! the `starting_after` value the api-key list exchanges. This one is DIFFERENT
//! on the wire: the whole token is base64url-encoded, it travels under
//! `?cursor=`, and there is no sort vocabulary beside it because the charges
//! walk has exactly one ordering.
//!
//! # The instant is RFC 3339, never an epoch count
//!
//! An instant is a `TIMESTAMPTZ(3)` in Postgres and a jiff `Timestamp` here, so
//! the token spells it the way the API spells every instant: jiff's standard
//! RFC 3339 form. `|` separates it from the id because RFC 3339 carries `:`.
//! A token naming an instant finer than the millisecond the column holds is
//! not one this daemon issued, and is refused like any other.
//!
//! # Opaque means opaque
//!
//! [`parse`] refuses everything malformed with one undifferentiated error.
//! Nothing says WHICH way a token was wrong: a cursor is this daemon's own
//! receipt handed back to it, and a parser that explained itself would be
//! describing an internal format to whoever is probing it.

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as BASE64;
use jiff::{Timestamp, Unit};

use crate::error;

/// The separator inside the decoded token.
const SEPARATOR: char = '|';

/// The longest identifier half a token may carry.
///
/// UUID-format identifiers are 36 characters; 128 is a generous cap that still
/// bounds what a decoded token can make this daemon hold.
const ID_MAX_LEN: usize = 128;

/// A decoded boundary: the last row the previous page showed.
///
/// The identifier's bound is declared here and proved by [`parse`], so a
/// boundary read from a token has always passed it.
#[derive(Debug, Clone, PartialEq, Eq, garde::Validate)]
pub struct Boundary {
    /// The boundary row's `created_at`.
    #[garde(skip)]
    pub created_at: Timestamp,
    /// The boundary row's identifier, breaking ties within one instant.
    #[garde(length(bytes, min = 1, max = ID_MAX_LEN))]
    pub id: String,
}

/// Renders the token a client resumes from after the row `(created_at, id)`.
///
/// The instant travels in jiff's RFC 3339 form at the millisecond the
/// `TIMESTAMPTZ(3)` column holds, so the boundary a token names is exactly the
/// row's.
#[must_use]
pub fn render(created_at: Timestamp, id: &str) -> String {
    BASE64.encode(format!("{created_at}{SEPARATOR}{id}"))
}

/// Reads a token this daemon issued.
///
/// # Errors
/// Refuses anything that is not base64url of `{RFC 3339 instant}|{id}`, with
/// the instant whole milliseconds and a non-empty id within its cap, as one
/// undifferentiated refusal.
pub fn parse(token: &str) -> crate::Result<Boundary> {
    let decoded = BASE64
        .decode(token)
        .map_err(|_undecodable| error::charges_cursor_invalid())?;
    let plain = String::from_utf8(decoded).map_err(|_not_text| error::charges_cursor_invalid())?;
    let (instant, id) = plain
        .split_once(SEPARATOR)
        .ok_or_else(error::charges_cursor_invalid)?;
    let created_at: Timestamp = instant
        .parse()
        .map_err(|_not_an_instant| error::charges_cursor_invalid())?;
    if created_at.round(Unit::Millisecond).ok() != Some(created_at) {
        return Err(error::charges_cursor_invalid());
    }
    garde::Unvalidated::new(Boundary {
        created_at,
        id: id.to_owned(),
    })
    .validate()
    .map(garde::Valid::into_inner)
    .map_err(|_out_of_bounds| error::charges_cursor_invalid())
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]
    use jiff::Timestamp;

    use super::{ID_MAX_LEN, parse, render};

    /// 2024-04-12T12:20:00Z, the instant every pinned token below names.
    const APRIL_2024: Timestamp = Timestamp::constant(1_712_924_400, 0);

    #[test]
    fn a_rendered_token_round_trips() {
        let token = render(APRIL_2024, "abc123");
        let boundary = parse(&token).expect("a token this daemon issued must parse");
        assert_eq!(boundary.created_at, APRIL_2024);
        assert_eq!(boundary.id, "abc123");
    }

    #[test]
    fn everything_that_is_not_a_token_is_refused_the_same_way() {
        use base64::Engine as _;
        use base64::engine::general_purpose::URL_SAFE_NO_PAD as BASE64;
        let long_id = format!("2024-04-12T12:20:00Z|{}", "a".repeat(129));
        // One case per way a token can be wrong: not base64url, base64 of no
        // separator, an empty id, a non-instant, an epoch count in place of
        // the instant, an instant finer than a millisecond, an id past the
        // cap. Each earns the SAME refusal, which is the opacity claim.
        for bad in [
            "!!not-valid-base64!!".to_owned(),
            BASE64.encode("no-separator"),
            BASE64.encode("2024-04-12T12:20:00Z|"),
            BASE64.encode("not-an-instant|abc"),
            BASE64.encode("1712924400000|abc"),
            BASE64.encode("2024-04-12T12:20:00.000001Z|abc"),
            BASE64.encode(long_id),
        ] {
            assert!(
                parse(&bad).is_err(),
                "{bad:?} is not a cursor this daemon issued"
            );
        }
    }

    #[test]
    fn test_cursor_id_bound_stays_one_refusal() {
        use base64::Engine as _;
        use base64::engine::general_purpose::URL_SAFE_NO_PAD as BASE64;
        // The id bound is garde's now; its refusal must still be the same one
        // a token that is not base64 at all earns, or the parser would start
        // describing its format to whoever probes it.
        let refusal = |token: &str| {
            parse(token)
                .err()
                .map(|error| (error.code(), error.detail()))
        };
        let malformed = refusal("!!not-valid-base64!!");
        let past_cap = BASE64.encode(format!(
            "2024-04-12T12:20:00Z|{}",
            "a".repeat(ID_MAX_LEN + 1)
        ));
        assert!(malformed.is_some());
        assert_eq!(refusal(&past_cap), malformed);
        let at_cap = render(APRIL_2024, &"a".repeat(ID_MAX_LEN));
        assert_eq!(refusal(&at_cap), None);
    }

    #[test]
    fn the_token_spells_the_instant_as_rfc3339() {
        use base64::Engine as _;
        use base64::engine::general_purpose::URL_SAFE_NO_PAD as BASE64;
        let token = render(APRIL_2024, "abc123");
        let plain =
            String::from_utf8(BASE64.decode(token).expect("base64url")).expect("the token is text");
        assert_eq!(plain, "2024-04-12T12:20:00Z|abc123");
    }

    #[test]
    fn a_millisecond_instant_round_trips_exactly() {
        let at = Timestamp::constant(1_712_924_400, 123_000_000);
        let boundary = parse(&render(at, "abc")).expect("a token this daemon issued must parse");
        assert_eq!(boundary.created_at, at);
    }
}
