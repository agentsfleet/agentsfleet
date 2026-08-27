//! The catalogue cursor: unpadded base64url of CANONICAL JSON.
//!
//! The port of `http/pagination.zig`'s struct-cursor codec, for the one §2
//! payload that uses it. A bare row id cannot carry this walk's position —
//! the sort keys are normalized expressions and the page is bound to the
//! filter and limit that issued it — so the cursor is a small struct, and
//! this module is the one place that turns it into an opaque string and back.
//!
//! # Canonical form, enforced by re-encoding
//!
//! Keys in declaration order, no extras, no whitespace, plain integers.
//! Rather than a bespoke strict parser, [`parse`] decodes permissively,
//! re-encodes canonically, and requires the result to equal the input byte
//! for byte — reordered keys, added whitespace, `1.0` for `1` all differ
//! after the round trip and are refused. A cursor a ZIG daemon issued is
//! canonical under the same rule, so it survives the cutover.
//!
//! # Two refusals, deliberately distinct
//!
//! [`Foreign::Malformed`] is "not a cursor this endpoint issued";
//! [`Foreign::VersionMismatch`] is "issued by a different payload
//! generation". Both answer `UZ-LIBRARY-001` — the split exists so a
//! deploy-boundary spike is legible in logs rather than hiding inside a
//! generic parse failure. Identity mismatch (`UZ-LIBRARY-002`) is NOT here:
//! only the handler knows the active filters and limit, so only the handler
//! can compare them.

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as BASE64;
use serde::{Deserialize, Serialize};

/// Cursor payload version, shared across every struct cursor issued.
///
/// `http/pagination.zig` owns the number; a decoded cursor carrying any
/// other value is refused, which is what stops a deploy from silently
/// reinterpreting yesterday's boundary.
pub const CURSOR_VERSION: u8 = 2;

/// The catalogue page's cursor payload, in the spec's fixed key order.
///
/// `provider` and `limit` bind the cursor to the query that issued it — the
/// handler refuses a resume under different ones as `UZ-LIBRARY-002`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cursor {
    /// The payload generation this cursor was issued under.
    pub v: u8,
    /// The boundary row's folded model key.
    pub display_key: String,
    /// The boundary row's folded provider key.
    pub vendor_key: String,
    /// The boundary row's id, breaking ties between folded twins.
    pub id: String,
    /// The provider filter the page was issued under, or `null`.
    pub provider: Option<String>,
    /// The page size the walk was issued under.
    pub limit: u32,
}

/// Why a token was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Foreign {
    /// Not a cursor this endpoint issued.
    Malformed,
    /// A cursor from a different payload generation.
    VersionMismatch,
}

/// Renders the opaque token for `cursor`.
///
/// Infallible: serializing a struct of strings and integers cannot fail, and
/// an API that pretended otherwise would put an error arm on every caller for
/// a path none of them can reach.
#[must_use]
pub fn render(cursor: &Cursor) -> String {
    let json = serde_json::to_string(cursor).unwrap_or_default();
    BASE64.encode(json)
}

/// Reads a token this daemon — or the Zig one — issued.
///
/// # Errors
/// Refuses anything that is not unpadded base64url of the canonical JSON
/// form, and a well-formed payload from another generation — distinctly, for
/// the module note's reason.
pub fn parse(token: &str) -> Result<Cursor, Foreign> {
    let json = BASE64
        .decode(token)
        .map_err(|_undecodable| Foreign::Malformed)?;
    let cursor: Cursor =
        serde_json::from_slice(&json).map_err(|_not_the_shape| Foreign::Malformed)?;
    // Canonicity: re-encode and require identical bytes. Reordered keys,
    // extra whitespace, a float spelling of an integer — each survives the
    // permissive parse above and each fails here.
    let canonical = serde_json::to_vec(&cursor).unwrap_or_default();
    if canonical != json {
        return Err(Foreign::Malformed);
    }
    if cursor.v != CURSOR_VERSION {
        return Err(Foreign::VersionMismatch);
    }
    Ok(cursor)
}

#[cfg(test)]
mod tests {
    use super::{CURSOR_VERSION, Cursor, Foreign, parse, render};

    fn sample() -> Cursor {
        Cursor {
            v: CURSOR_VERSION,
            display_key: "claude-sonnet-5".to_owned(),
            vendor_key: "anthropic".to_owned(),
            id: "0195b4ba-8d3a-7f13-8abc-cd0000000002".to_owned(),
            provider: None,
            limit: 50,
        }
    }

    #[test]
    fn a_rendered_token_round_trips() {
        let token = render(&sample());
        assert_eq!(parse(&token), Ok(sample()));
    }

    #[test]
    fn the_wire_form_is_the_zig_codec_s() {
        // Pinned bytes: what `std.json.Stringify` emits for this payload,
        // base64url without padding. A client holding a Zig-issued cursor
        // must be able to spend it here mid-cutover, so the encoding is a
        // wire fact rather than a choice.
        use base64::Engine as _;
        let json = "{\"v\":2,\"display_key\":\"claude-sonnet-5\",\
\"vendor_key\":\"anthropic\",\"id\":\"0195b4ba-8d3a-7f13-8abc-cd0000000002\",\
\"provider\":null,\"limit\":50}";
        let token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(json);
        assert_eq!(render(&sample()), token);
        assert_eq!(parse(&token), Ok(sample()));
    }

    #[test]
    fn everything_non_canonical_is_refused_the_same_way() {
        use base64::Engine as _;
        let encode = |json: &str| base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(json);
        // One case per way a token can be wrong: not base64url, not JSON,
        // reordered keys, an extra key, whitespace, a float-spelled integer.
        for bad in [
            "!!not-base64!!".to_owned(),
            encode("hello"),
            encode(
                "{\"v\":2,\"vendor_key\":\"a\",\"display_key\":\"d\",\"id\":\"i\",\
\"provider\":null,\"limit\":50}",
            ),
            encode(
                "{\"v\":2,\"display_key\":\"d\",\"vendor_key\":\"a\",\"id\":\"i\",\
\"provider\":null,\"limit\":50,\"extra\":true}",
            ),
            encode(
                "{\"v\": 2,\"display_key\":\"d\",\"vendor_key\":\"a\",\"id\":\"i\",\
\"provider\":null,\"limit\":50}",
            ),
            encode(
                "{\"v\":2,\"display_key\":\"d\",\"vendor_key\":\"a\",\"id\":\"i\",\
\"provider\":null,\"limit\":50.0}",
            ),
        ] {
            assert_eq!(
                parse(&bad),
                Err(Foreign::Malformed),
                "{bad:?} is not a cursor this daemon issued"
            );
        }
    }

    #[test]
    fn another_generation_is_refused_distinctly() {
        let old = Cursor { v: 1, ..sample() };
        let token = render(&old);
        assert_eq!(parse(&token), Err(Foreign::VersionMismatch));
    }
}
