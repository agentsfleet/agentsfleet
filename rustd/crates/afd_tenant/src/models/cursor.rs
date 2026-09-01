//! The catalogue cursor: which fields a `/v1/models` page resumes from.
//!
//! A bare row id cannot carry this walk's position — the sort keys are
//! normalized expressions and the page is bound to the filter and limit that
//! issued it — so the boundary is a small struct rather than one of
//! [`afd_core::paging::Cursor`]'s two scalar forms.
//!
//! The ENCODING is not here. Canonical JSON, unpadded base64url, the
//! re-encode that refuses a reordered or whitespaced token, and the generation
//! check all live in [`afd_core::paging::struct_cursor`], which the tenant
//! registry's page uses for the same reason this one does. What stays here is
//! the payload: its fields, their order — which IS the canonical key order —
//! and the two spellings this daemon renders and reads.

use afd_core::paging::struct_cursor::{self, StructCursor};
use serde::{Deserialize, Serialize};

pub use afd_core::paging::struct_cursor::Foreign;

/// Cursor payload version, shared across every struct cursor issued.
///
/// Re-exported rather than declared: `http/pagination.zig` owns one number for
/// every struct cursor, and a second declaration here is how two payloads end
/// up on different generations.
pub const CURSOR_VERSION: u8 = struct_cursor::VERSION;

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

impl StructCursor for Cursor {
    fn generation(&self) -> u8 {
        self.v
    }
}

/// Renders the opaque token for `cursor`.
#[must_use]
pub fn render(cursor: &Cursor) -> String {
    struct_cursor::render(cursor)
}

/// Reads a token this daemon — or the Zig one — issued.
///
/// # Errors
/// Refuses anything that is not this payload in canonical form, and a
/// well-formed payload from another generation — distinctly, so a
/// deploy-boundary spike is legible in logs.
pub fn parse(token: &str) -> Result<Cursor, Foreign> {
    struct_cursor::parse(token)
}

#[cfg(test)]
mod tests {
    use super::{CURSOR_VERSION, Cursor, parse, render};

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
        // Pinned bytes: what `std.json.Stringify` emits for THIS payload,
        // base64url without padding. A client holding a Zig-issued cursor must
        // be able to spend it here mid-cutover, so the field order is a wire
        // fact rather than a choice — which is why the pin lives beside the
        // struct and not with the codec that encodes it.
        use base64::Engine as _;
        let json = "{\"v\":2,\"display_key\":\"claude-sonnet-5\",\
\"vendor_key\":\"anthropic\",\"id\":\"0195b4ba-8d3a-7f13-8abc-cd0000000002\",\
\"provider\":null,\"limit\":50}";
        let token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(json);
        assert_eq!(render(&sample()), token);
        assert_eq!(parse(&token), Ok(sample()));
    }
}
