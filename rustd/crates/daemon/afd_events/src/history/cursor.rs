//! Where a page of history resumes from.
//!
//! # What the token carries
//!
//! The boundary pair, `(created_at, event_id)`, as `{instant}|{event_id}` in
//! base64url without padding: the instant in RFC 3339 exactly as the API
//! serializes one (`…:49Z`, `…:49.123Z`), never as epoch digits, because an
//! instant is never a number anywhere this product speaks. The separator is a
//! character no RFC 3339 instant contains, so the first one ends the instant
//! whatever the event id holds.
//!
//! # Why this is not [`af_core::paging::Cursor`]
//!
//! That type exists, it is keyset, and it carries the same two fields in the
//! clear. The events endpoints wrap the pair, which keeps the token opaque to a
//! client that would otherwise parse it.

use af_core::id::Uuid7;
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as BASE64;

use jiff::Timestamp;

use crate::error::{Result, cursor_malformed};

/// The character between the two fields, inside the encoded payload: one no
/// RFC 3339 instant contains.
const FIELD_SEPARATOR: char = '|';

/// The boundary a page resumes strictly after, newest-first.
///
/// The identifier's bound is declared here and proved by [`Cursor::decode`],
/// so a cursor a client sent back has always passed it.
#[derive(Debug, Clone, PartialEq, Eq, garde::Validate)]
pub struct Cursor {
    /// The `created_at` of the last row on the previous page.
    #[garde(skip)]
    pub created_at: Timestamp,
    /// The `event_id` of that row, breaking ties within one millisecond.
    #[garde(skip)]
    pub event_id: Uuid7,
}

impl Cursor {
    /// The cursor a page ending on this row hands back.
    #[must_use]
    pub fn after(created_at: Timestamp, event_id: &Uuid7) -> Self {
        Self {
            created_at,
            event_id: event_id.to_owned(),
        }
    }

    /// The opaque string a client is given.
    ///
    /// Base64url without padding — a `=` would have to be percent-encoded in
    /// the query string it comes back in.
    #[must_use]
    pub fn encode(&self) -> String {
        BASE64.encode(format!(
            "{}{FIELD_SEPARATOR}{}",
            self.created_at, self.event_id
        ))
    }

    /// The cursor a client sent back, or a refusal.
    ///
    /// Every failure answers the same [`cursor_malformed`] and says
    /// nothing more. A parser that distinguished "not base64" from "no
    /// separator" from "identifier too long" would be describing this
    /// daemon's internal format to whoever was probing it.
    ///
    /// # Errors
    /// [`cursor_malformed`] for anything this daemon did not mint.
    pub fn decode(raw: &str) -> Result<Self> {
        let decoded = BASE64.decode(raw).map_err(|_decode| cursor_malformed())?;
        let plain = String::from_utf8(decoded).map_err(|_utf8| cursor_malformed())?;
        let (head, id) = plain
            .split_once(FIELD_SEPARATOR)
            .ok_or_else(cursor_malformed)?;
        let created_at: Timestamp = head.parse().map_err(|_instant| cursor_malformed())?;
        let event_id = Uuid7::parse(id).map_err(|_id| cursor_malformed())?;
        garde::Unvalidated::new(Self::after(created_at, &event_id))
            .validate()
            .map(garde::Valid::into_inner)
            .map_err(|_out_of_bounds| cursor_malformed())
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]

    use super::*;

    /// The cursor `raw` decodes to.
    ///
    /// A helper rather than an inline `expect` at each site because `Error`
    /// cannot derive `PartialEq` — it carries an `sqlx::Error` — so the
    /// `assert_eq!(.., Ok(..))` shape `af_core::paging` uses for its own
    /// cursor is unavailable here.
    fn decoded(raw: &str) -> Cursor {
        Cursor::decode(raw).expect("a cursor this test built or spelled out")
    }

    /// The boundary every case pages from: a millisecond instant, as a row
    /// stores one.
    const AT: Timestamp = Timestamp::constant(1_735_689_600, 123_000_000);

    /// The event a page ends on.
    fn event() -> Uuid7 {
        Uuid7::parse("0190d5c2-7e1a-7000-8000-000000000001").expect("a canonical UUIDv7")
    }

    #[test]
    fn round_trips_through_the_wire_form() {
        let cursor = Cursor::after(AT, &event());
        assert_eq!(decoded(&cursor.encode()), cursor);
    }

    #[test]
    fn the_token_spells_the_instant_as_rfc3339() {
        // Spelled out as bytes rather than built by `encode`: the payload is a
        // data format, so its shape is asserted, not re-derived.
        assert_eq!(
            Cursor::after(AT, &event()).encode(),
            "MjAyNS0wMS0wMVQwMDowMDowMC4xMjNafDAxOTBkNWMyLTdlMWEtNzAwMC04MDAwLTAwMDAwMDAwMDAwMQ"
        );
    }

    #[test]
    fn an_identifier_that_is_not_a_uuidv7_is_refused() {
        // Every event id is a UUIDv7, so a cursor naming anything else was not
        // minted here.
        let raw = BASE64.encode("2025-01-01T00:00:00.123Z|cron:a|b");
        let refused = Cursor::decode(&raw).err();
        assert!(refused.is_some());
    }

    #[test]
    fn refuses_what_this_daemon_did_not_mint() {
        for raw in [
            "",                                                // empty
            "not-base64!!",                                    // not the alphabet
            &BASE64.encode("no-separator"),                    // decodes, no separator
            &BASE64.encode("1735689600000|01HZQ"),             // epoch digits, not an instant
            &BASE64.encode("2025-01-01T00:00:00Z|"),           // empty identifier
            &BASE64.encode("2025-01-01T00:00:00Z|01HZQ8P0X3"), // not a UUIDv7
        ] {
            let refused = Cursor::decode(raw);
            assert!(refused.is_err(), "accepted {raw:?}");
            assert_eq!(
                refused.err().map(|error| error.code()),
                Some(af_core::error_code::INVALID_REQUEST),
                "{raw:?} was refused as something other than a bad cursor"
            );
        }
    }

    #[test]
    fn the_refusal_names_no_cause() {
        // RULE from docs/RUST_ERROR_STANDARD.md §4: a variant holding only data
        // has no source, and inventing one would put a Postgres error on a path
        // Postgres never saw.
        let refusal = Cursor::decode("!!").expect_err("two bangs are not a cursor");
        assert!(std::error::Error::source(&refusal).is_none());
    }
}
