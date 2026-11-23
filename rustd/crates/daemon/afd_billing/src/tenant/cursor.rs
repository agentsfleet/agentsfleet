//! The charges endpoint's opaque cursor: the boundary row's `created_at` and
//! `id`, as an [`af_core::paging::struct_cursor`] payload.
//!
//! # The encoding is the shared one
//!
//! Canonical JSON, unpadded base64url, the re-encode that refuses a reordered
//! or whitespaced token, and the generation check all live in
//! [`af_core::paging::struct_cursor`]. What stays here is the payload, whose
//! field order is the canonical key order, and the millisecond check below.
//!
//! # The instant is RFC 3339, never an epoch count
//!
//! An instant is a `TIMESTAMPTZ(3)` in Postgres and a jiff `Timestamp` here, so
//! the token spells it the way the API spells every instant: jiff's standard
//! RFC 3339 form. A token naming an instant finer than the millisecond the
//! column holds is not one this daemon issued, and is refused like any other.
//!
//! # The id is parsed, not length-checked
//!
//! The id decodes into a [`Uuid7`]. A token whose id is not one is refused at
//! this boundary, before the statement that binds it as `uuid` could fail
//! inside Postgres and surface as a server fault.
//!
//! # Opaque means opaque
//!
//! [`parse`] refuses everything malformed, another generation included, with
//! one undifferentiated error: a cursor is this daemon's own receipt handed
//! back to it, and a parser that explained itself would be describing an
//! internal format to whoever is probing it.

use af_core::id::Uuid7;
use af_core::paging::struct_cursor::{self, StructCursor};
use jiff::{Timestamp, Unit};
use serde::{Deserialize, Serialize};

use crate::error;

/// A decoded boundary: the last row the previous page showed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Boundary {
    /// The boundary row's `created_at`.
    pub created_at: Timestamp,
    /// The boundary row's identifier, breaking ties within one instant.
    pub id: Uuid7,
}

/// The token's payload, in its canonical key order.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Payload {
    v: u8,
    created_at: Timestamp,
    id: Uuid7,
}

impl StructCursor for Payload {
    fn generation(&self) -> u8 {
        self.v
    }
}

/// Renders the token a client resumes from after the row `(created_at, id)`.
///
/// The instant travels in jiff's RFC 3339 form at the millisecond the
/// `TIMESTAMPTZ(3)` column holds, so the boundary a token names is exactly the
/// row's.
#[must_use]
pub fn render(created_at: Timestamp, id: &Uuid7) -> String {
    struct_cursor::render(&Payload {
        v: struct_cursor::VERSION,
        created_at,
        id: id.clone(),
    })
}

/// Reads a token this daemon issued.
///
/// # Errors
/// Refuses anything that is not a canonical payload of this generation, with
/// the instant whole milliseconds and the id a canonical `UUIDv7`, as one
/// undifferentiated refusal.
pub fn parse(token: &str) -> crate::Result<Boundary> {
    let Payload { created_at, id, .. } =
        struct_cursor::parse(token).map_err(|_foreign| error::charges_cursor_invalid())?;
    if created_at.round(Unit::Millisecond).ok() != Some(created_at) {
        return Err(error::charges_cursor_invalid());
    }
    Ok(Boundary { created_at, id })
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]
    use af_core::id::Uuid7;
    use base64::Engine as _;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD as BASE64;
    use jiff::Timestamp;

    use super::{parse, render};

    /// 2024-04-12T12:20:00Z, the instant every pinned token below names.
    const APRIL_2024: Timestamp = Timestamp::constant(1_712_924_400, 0);

    /// A row id in the canonical `UUIDv7` spelling.
    const ROW_ID: &str = "01924f4e-0000-7000-8000-00000000c4a1";

    fn row_id() -> Uuid7 {
        Uuid7::parse(ROW_ID).expect("a canonical UUIDv7")
    }

    #[test]
    fn a_rendered_token_round_trips() {
        let token = render(APRIL_2024, &row_id());
        let boundary = parse(&token).expect("a token this daemon issued must parse");
        assert_eq!(boundary.created_at, APRIL_2024);
        assert_eq!(boundary.id.as_str(), ROW_ID);
    }

    #[test]
    fn everything_that_is_not_a_token_is_refused_the_same_way() {
        // One case per way a token can be wrong: not base64url, not the
        // payload, an earlier pipe-separated token, an epoch count in place of
        // the instant, an instant finer than a millisecond, another generation,
        // and an id that is not a canonical UUIDv7. Each earns the SAME
        // refusal, which is the opacity claim.
        let refusal = |token: &str| {
            parse(token)
                .err()
                .map(|error| (error.code(), error.detail()))
        };
        let malformed = refusal("!!not-valid-base64!!");
        assert!(malformed.is_some());
        let payload = |v: u8, at: &str, id: &str| {
            BASE64.encode(format!("{{\"v\":{v},\"created_at\":{at},\"id\":\"{id}\"}}"))
        };
        for bad in [
            BASE64.encode("no-payload"),
            BASE64.encode(format!("2024-04-12T12:20:00Z|{ROW_ID}")),
            payload(2, "1712924400000", ROW_ID),
            payload(2, "\"2024-04-12T12:20:00.000001Z\"", ROW_ID),
            payload(1, "\"2024-04-12T12:20:00Z\"", ROW_ID),
            payload(2, "\"2024-04-12T12:20:00Z\"", "x"),
            payload(
                2,
                "\"2024-04-12T12:20:00Z\"",
                "01924F4E-0000-7000-8000-00000000C4A1",
            ),
            payload(
                2,
                "\"2024-04-12T12:20:00Z\"",
                "01924f4e-0000-4000-8000-00000000c4a1",
            ),
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
        let token = render(APRIL_2024, &row_id());
        let plain =
            String::from_utf8(BASE64.decode(token).expect("base64url")).expect("the token is text");
        assert_eq!(
            plain,
            format!("{{\"v\":2,\"created_at\":\"2024-04-12T12:20:00Z\",\"id\":\"{ROW_ID}\"}}")
        );
    }

    #[test]
    fn a_millisecond_instant_round_trips_exactly() {
        let at = Timestamp::constant(1_712_924_400, 123_000_000);
        let boundary =
            parse(&render(at, &row_id())).expect("a token this daemon issued must parse");
        assert_eq!(boundary.created_at, at);
    }
}
