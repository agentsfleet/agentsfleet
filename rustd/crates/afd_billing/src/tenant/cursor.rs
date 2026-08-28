//! The charges endpoint's opaque cursor: base64url of `{recorded_at}:{id}`.
//!
//! # A second cursor format, not a variant of the first
//!
//! The tenant plane already has a keyset cursor — [`afd_core::paging::Cursor`],
//! the `starting_after` value the api-key list exchanges. This one is DIFFERENT
//! on the wire: the whole token is base64url-encoded, it travels under
//! `?cursor=`, and there is no sort vocabulary beside it because the charges
//! walk has exactly one ordering. Two formats is not a design anyone would
//! choose fresh; it is what the Zig daemon serves, and a client holding a
//! cursor issued by one binary must be able to spend it against the other
//! mid-cutover. Unifying them is a post-cutover change, made when there is one
//! daemon left to change.
//!
//! # Opaque means opaque
//!
//! [`parse`] refuses everything malformed with one undifferentiated error.
//! Nothing says WHICH way a token was wrong: a cursor is this daemon's own
//! receipt handed back to it, and a parser that explained itself would be
//! describing an internal format to whoever is probing it.

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as BASE64;

use crate::error;

/// The separator inside the decoded token.
const SEPARATOR: char = ':';

/// The longest identifier half a token may carry.
///
/// UUID-format identifiers are 36 characters; 128 is the same generous cap the
/// Zig codec applies, kept because a cursor crossing binaries mid-cutover must
/// be judged by the same rule on both sides.
const ID_MAX_LEN: usize = 128;

/// A decoded boundary: the last row the previous page showed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Boundary {
    /// The boundary row's `created_at`.
    pub recorded_at: i64,
    /// The boundary row's identifier, breaking ties within one instant.
    pub id: String,
}

/// Renders the token a client resumes from after the row `(recorded_at, id)`.
#[must_use]
pub fn render(recorded_at: i64, id: &str) -> String {
    BASE64.encode(format!("{recorded_at}{SEPARATOR}{id}"))
}

/// Reads a token this daemon — or the Zig one — issued.
///
/// # Errors
/// Refuses anything that is not base64url of `{integer}:{id}` with a non-empty
/// id within its cap, as one undifferentiated refusal.
pub fn parse(token: &str) -> crate::Result<Boundary> {
    let decoded = BASE64
        .decode(token)
        .map_err(|_undecodable| error::charges_cursor_invalid())?;
    let plain = String::from_utf8(decoded).map_err(|_not_text| error::charges_cursor_invalid())?;
    let (instant, id) = plain
        .split_once(SEPARATOR)
        .ok_or_else(error::charges_cursor_invalid)?;
    let recorded_at = instant
        .parse()
        .map_err(|_not_numeric| error::charges_cursor_invalid())?;
    if id.is_empty() || id.len() > ID_MAX_LEN {
        return Err(error::charges_cursor_invalid());
    }
    Ok(Boundary {
        recorded_at,
        id: id.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]
    use super::{parse, render};

    #[test]
    fn a_rendered_token_round_trips() {
        let token = render(1_712_924_400_000, "abc123");
        let boundary = parse(&token).expect("a token this daemon issued must parse");
        assert_eq!(boundary.recorded_at, 1_712_924_400_000);
        assert_eq!(boundary.id, "abc123");
    }

    #[test]
    fn everything_that_is_not_a_token_is_refused_the_same_way() {
        use base64::Engine as _;
        use base64::engine::general_purpose::URL_SAFE_NO_PAD as BASE64;
        let long_id = format!("1712924400000:{}", "a".repeat(129));
        // One case per way a token can be wrong: not base64url, base64 of
        // no separator, an empty id, a non-numeric instant, an id past the
        // cap. Each earns the SAME refusal, which is the opacity claim.
        for bad in [
            "!!not-valid-base64!!".to_owned(),
            BASE64.encode("no-separator"),
            BASE64.encode("1712924400000:"),
            BASE64.encode("not-a-number:abc"),
            BASE64.encode(long_id),
        ] {
            assert!(
                parse(&bad).is_err(),
                "{bad:?} is not a cursor this daemon issued"
            );
        }
    }

    #[test]
    fn the_wire_form_matches_the_zig_codec() {
        // Pinned bytes: base64url-no-pad of "1712924400000:abc123", the exact
        // value `fleet_telemetry_cursor.zig`'s round-trip test builds. A client
        // holding a Zig-issued cursor must be able to spend it here
        // mid-cutover, so the encoding is a wire fact rather than a choice.
        assert_eq!(
            render(1_712_924_400_000, "abc123"),
            "MTcxMjkyNDQwMDAwMDphYmMxMjM"
        );
    }
}
