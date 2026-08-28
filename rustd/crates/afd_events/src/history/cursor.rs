//! Where a page of history resumes from.
//!
//! # Why this is not [`afd_core::paging::Cursor`]
//!
//! That type exists, it is keyset, and it carries the same two fields. It also
//! spells a timestamp boundary `{millis}:{id}` in the CLEAR, because that is
//! what `keyset_cursor.zig` spells — and the events endpoints do not use
//! `keyset_cursor.zig`. They use `fleet_events_filter.zig`, which wraps the
//! whole pair in base64url.
//!
//! Two cursor formats in one product is not a thing to be pleased about, and it
//! was inherited rather than chosen. What settles which one this crate emits is
//! `docs/REST_API_DESIGN_GUIDELINES.md` §9: a cursor is already exposed on
//! these paths in production, a dashboard holds one across a deploy, and
//! re-spelling it inside `/v1` is the breaking change that section forbids.
//! Converging the two is a `/v2` change, and it is recorded as one.

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as BASE64;

use crate::error::Error;

/// The character between the two fields, inside the encoded payload.
const FIELD_SEPARATOR: char = ':';

/// The longest event identifier a cursor will carry.
///
/// `CURSOR_EVENT_ID_MAX_LEN`, mirrored. The bound is on the DECODED text, so a
/// caller cannot spend the server's memory by sending a long base64 string that
/// decodes to something this reader would then hold.
const EVENT_ID_MAX_LEN: usize = 128;

/// The boundary a page resumes strictly after, newest-first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cursor {
    /// The `created_at` of the last row on the previous page.
    pub created_at: i64,
    /// The `event_id` of that row, breaking ties within one millisecond.
    pub event_id: String,
}

impl Cursor {
    /// The cursor a page ending on this row hands back.
    #[must_use]
    pub fn after(created_at: i64, event_id: &str) -> Self {
        Self {
            created_at,
            event_id: event_id.to_owned(),
        }
    }

    /// The opaque string a client is given.
    ///
    /// Base64url without padding, exactly as `makeCursor` writes it — a `=`
    /// would have to be percent-encoded in the query string it comes back in.
    #[must_use]
    pub fn encode(&self) -> String {
        BASE64.encode(format!(
            "{}{FIELD_SEPARATOR}{}",
            self.created_at, self.event_id
        ))
    }

    /// The cursor a client sent back, or a refusal.
    ///
    /// Every failure answers the same [`Error::CursorMalformed`] and says
    /// nothing more. A parser that distinguished "not base64" from "no
    /// separator" from "identifier too long" would be describing this
    /// daemon's internal format to whoever was probing it.
    ///
    /// # Errors
    /// [`Error::CursorMalformed`] for anything this daemon did not mint.
    pub fn decode(raw: &str) -> Result<Self, Error> {
        let decoded = BASE64
            .decode(raw)
            .map_err(|_decode| Error::CursorMalformed)?;
        let plain = String::from_utf8(decoded).map_err(|_utf8| Error::CursorMalformed)?;
        let (head, id) = plain
            .split_once(FIELD_SEPARATOR)
            .ok_or(Error::CursorMalformed)?;
        let created_at: i64 = head.parse().map_err(|_digits| Error::CursorMalformed)?;
        if id.is_empty() || id.len() > EVENT_ID_MAX_LEN {
            return Err(Error::CursorMalformed);
        }
        Ok(Self {
            created_at,
            event_id: id.to_owned(),
        })
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
    /// `assert_eq!(.., Ok(..))` shape `afd_core::paging` uses for its own
    /// cursor is unavailable here.
    fn decoded(raw: &str) -> Cursor {
        Cursor::decode(raw).expect("a cursor this test built or the Zig issued")
    }

    #[test]
    fn round_trips_through_the_wire_form() {
        let cursor = Cursor::after(1_735_689_600_000, "01HZQ8P0X3");
        assert_eq!(decoded(&cursor.encode()), cursor);
    }

    #[test]
    fn reads_the_zig_daemons_bytes() {
        // base64url of `1735689600000:01HZQ8P0X3` — a cursor the OTHER daemon
        // issued. This is the assertion that makes the format a data format
        // rather than an implementation detail: a dashboard holding this
        // string across a deploy must land on either binary and page on.
        let issued_by_zig = "MTczNTY4OTYwMDAwMDowMUhaUThQMFgz";
        assert_eq!(
            decoded(issued_by_zig),
            Cursor::after(1_735_689_600_000, "01HZQ8P0X3")
        );
    }

    #[test]
    fn a_negative_timestamp_survives_the_round_trip() {
        // Pre-epoch is not a case the product produces, but the field is a
        // signed integer and a parser that lost the sign would silently page
        // from the wrong end.
        let cursor = Cursor::after(-42, "e");
        assert_eq!(decoded(&cursor.encode()), cursor);
    }

    #[test]
    fn refuses_what_this_daemon_did_not_mint() {
        for raw in [
            "",                                 // empty
            "not-base64!!",                     // not the alphabet
            &BASE64.encode("no-separator"),     // decodes, but has no colon
            &BASE64.encode("notanumber:01HZQ"), // separator, unparseable head
            &BASE64.encode("1735689600000:"),   // empty identifier
            &BASE64.encode(format!("1:{}", "x".repeat(EVENT_ID_MAX_LEN + 1))),
        ] {
            assert!(
                matches!(Cursor::decode(raw), Err(Error::CursorMalformed)),
                "accepted {raw:?}"
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
