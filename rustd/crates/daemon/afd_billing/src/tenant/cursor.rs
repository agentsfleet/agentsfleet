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
//! # The id is parsed, not length-checked
//!
//! The id half is decoded into a [`Uuid7`] here. A token whose id is not one
//! is refused at this boundary, before the statement that binds it as `uuid`
//! could fail inside Postgres and surface as a server fault.
//!
//! # Opaque means opaque
//!
//! [`parse`] refuses everything malformed with one undifferentiated error.
//! Nothing says WHICH way a token was wrong: a cursor is this daemon's own
//! receipt handed back to it, and a parser that explained itself would be
//! describing an internal format to whoever is probing it.

use afd_core::id::Uuid7;
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as BASE64;
use jiff::{Timestamp, Unit};

use crate::error;

/// The separator inside the decoded token.
const SEPARATOR: char = '|';

/// A decoded boundary: the last row the previous page showed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Boundary {
    /// The boundary row's `created_at`.
    pub created_at: Timestamp,
    /// The boundary row's identifier, breaking ties within one instant.
    pub id: Uuid7,
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
/// the instant whole milliseconds and the id a canonical `UUIDv7`, as one
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
    let id = Uuid7::parse(id).map_err(|_not_an_id| error::charges_cursor_invalid())?;
    Ok(Boundary { created_at, id })
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]
    use base64::Engine as _;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD as BASE64;
    use jiff::Timestamp;

    use super::{parse, render};

    /// 2024-04-12T12:20:00Z, the instant every pinned token below names.
    const APRIL_2024: Timestamp = Timestamp::constant(1_712_924_400, 0);

    /// A row id in the canonical `UUIDv7` spelling.
    const ROW_ID: &str = "01924f4e-0000-7000-8000-00000000c4a1";

    #[test]
    fn a_rendered_token_round_trips() {
        let token = render(APRIL_2024, ROW_ID);
        let boundary = parse(&token).expect("a token this daemon issued must parse");
        assert_eq!(boundary.created_at, APRIL_2024);
        assert_eq!(boundary.id.as_str(), ROW_ID);
    }

    #[test]
    fn everything_that_is_not_a_token_is_refused_the_same_way() {
        // One case per way a token can be wrong: not base64url, base64 of no
        // separator, an empty id, a non-instant, an epoch count in place of
        // the instant, an instant finer than a millisecond, and an id that is
        // not a canonical UUIDv7 (free text, past any length, uppercase, a
        // version-4 UUID). Each earns the SAME refusal, which is the opacity
        // claim, and none reaches the statement that binds the id as `uuid`.
        let refusal = |token: &str| {
            parse(token)
                .err()
                .map(|error| (error.code(), error.detail()))
        };
        let malformed = refusal("!!not-valid-base64!!");
        assert!(malformed.is_some());
        for bad in [
            BASE64.encode("no-separator"),
            BASE64.encode("2024-04-12T12:20:00Z|"),
            BASE64.encode(format!("not-an-instant|{ROW_ID}")),
            BASE64.encode(format!("1712924400000|{ROW_ID}")),
            BASE64.encode(format!("2024-04-12T12:20:00.000001Z|{ROW_ID}")),
            BASE64.encode("2024-04-12T12:20:00Z|x"),
            BASE64.encode(format!("2024-04-12T12:20:00Z|{}", "a".repeat(129))),
            BASE64.encode("2024-04-12T12:20:00Z|01924F4E-0000-7000-8000-00000000C4A1"),
            BASE64.encode("2024-04-12T12:20:00Z|01924f4e-0000-4000-8000-00000000c4a1"),
        ] {
            assert_eq!(
                refusal(&bad),
                malformed,
                "{bad:?} is not a cursor this daemon issued"
            );
        }
    }

    #[test]
    fn the_token_spells_the_instant_as_rfc3339() {
        let token = render(APRIL_2024, ROW_ID);
        let plain =
            String::from_utf8(BASE64.decode(token).expect("base64url")).expect("the token is text");
        assert_eq!(plain, format!("2024-04-12T12:20:00Z|{ROW_ID}"));
    }

    #[test]
    fn a_millisecond_instant_round_trips_exactly() {
        let at = Timestamp::constant(1_712_924_400, 123_000_000);
        let boundary = parse(&render(at, ROW_ID)).expect("a token this daemon issued must parse");
        assert_eq!(boundary.created_at, at);
    }
}
