//! Where a page of history resumes from.
//!
//! # What the token carries
//!
//! The boundary pair, `(created_at, turn_id)`, as an
//! [`af_core::paging::struct_cursor`] payload: canonical JSON in unpadded
//! base64url, the instant in RFC 3339 exactly as the API serializes one
//! (`…:49Z`, `…:49.123Z`), never as epoch digits, because an instant is never
//! a number anywhere this product speaks.
//!
//! # Why this is not [`af_core::paging::Cursor`]
//!
//! That type exists, it is keyset, and it carries the same two fields in the
//! clear. The events endpoints wrap the pair, which keeps the token opaque to a
//! client that would otherwise parse it.

use af_core::id::Uuid7;
use af_core::paging::struct_cursor::{self, StructCursor};
use jiff::Timestamp;
use serde::{Deserialize, Serialize};

use crate::view::error::{Result, cursor_malformed};

/// The boundary a page resumes strictly after, newest-first.
///
/// [`Cursor::decode`] parses both fields, so a cursor a client sent back has
/// always been a well-formed instant and identifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cursor {
    /// The `created_at` of the last row on the previous page.
    pub created_at: Timestamp,
    /// The `turn_id` of that row, breaking ties within one millisecond.
    pub turn_id: Uuid7,
}

/// The token's payload, in its canonical key order.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Payload {
    v: u8,
    created_at: Timestamp,
    turn_id: Uuid7,
}

impl StructCursor for Payload {
    fn generation(&self) -> u8 {
        self.v
    }
}

impl Cursor {
    /// The cursor a page ending on this row hands back.
    #[must_use]
    pub fn after(created_at: Timestamp, turn_id: &Uuid7) -> Self {
        Self {
            created_at,
            turn_id: turn_id.to_owned(),
        }
    }

    /// The opaque string a client is given.
    #[must_use]
    pub fn encode(&self) -> String {
        struct_cursor::render(&Payload {
            v: struct_cursor::VERSION,
            created_at: self.created_at,
            turn_id: self.turn_id.clone(),
        })
    }

    /// The cursor a client sent back, or a refusal.
    ///
    /// Every failure, another generation included, answers the same
    /// [`cursor_malformed`] and says nothing more. A parser that distinguished
    /// "not base64" from "not the payload" would be describing this daemon's
    /// internal format to whoever was probing it.
    ///
    /// # Errors
    /// [`cursor_malformed`] for anything this daemon did not mint.
    pub fn decode(raw: &str) -> Result<Self> {
        let Payload {
            created_at,
            turn_id,
            ..
        } = struct_cursor::parse(raw).map_err(|_foreign| cursor_malformed())?;
        Ok(Self {
            created_at,
            turn_id,
        })
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]

    use base64::Engine as _;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD as BASE64;

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
            BASE64.encode(
                "{\"v\":2,\"created_at\":\"2025-01-01T00:00:00.123Z\",\
\"turn_id\":\"0190d5c2-7e1a-7000-8000-000000000001\"}"
            )
        );
    }

    #[test]
    fn an_identifier_that_is_not_a_uuidv7_is_refused() {
        // Every event id is a UUIDv7, so a cursor naming anything else was not
        // minted here.
        let raw = BASE64.encode(
            "{\"v\":2,\"created_at\":\"2025-01-01T00:00:00.123Z\",\"turn_id\":\"cron:a|b\"}",
        );
        let refused = Cursor::decode(&raw).err();
        assert!(refused.is_some());
    }

    #[test]
    fn refuses_what_this_daemon_did_not_mint() {
        for raw in [
            "",                                                // empty
            "not-base64!!",                                    // not the alphabet
            &BASE64.encode("no-payload"),                      // decodes, not the payload
            &BASE64.encode("2025-01-01T00:00:00Z|0190d5c2-7e1a-7000-8000-000000000001"), // the earlier pipe form
            &BASE64.encode("{\"v\":2,\"created_at\":1735689600000,\"turn_id\":\"0190d5c2-7e1a-7000-8000-000000000001\"}"), // epoch digits
            &BASE64.encode("{\"v\":1,\"created_at\":\"2025-01-01T00:00:00Z\",\"turn_id\":\"0190d5c2-7e1a-7000-8000-000000000001\"}"), // another generation
            &BASE64.encode("{\"v\":2,\"created_at\":\"2025-01-01T00:00:00Z\",\"turn_id\":\"01HZQ8P0X3\"}"), // not a UUIDv7
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
        // RS-NOCAUSE: a variant holding only data
        // has no source, and inventing one would put a Postgres error on a path
        // Postgres never saw.
        let refusal = Cursor::decode("!!").expect_err("two bangs are not a cursor");
        assert!(std::error::Error::source(&refusal).is_none());
    }
}
