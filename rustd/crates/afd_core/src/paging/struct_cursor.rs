//! The other cursor form: a small payload, canonically encoded, opaque on the wire.
//!
//! [`super::Cursor`] carries a boundary and an id, and that is enough for a
//! walk whose ORDER BY is fixed by a [`SortOrder`](super::SortOrder). Some
//! pages need more: the catalogue's sort keys are normalized expressions, and
//! the tenant registry binds a walk to the tenant and page size that issued it
//! so a token cannot be replayed against either. Those payloads are structs,
//! and this is the one place that turns one into a string and back.
//!
//! # Canonical form, enforced by re-encoding
//!
//! Keys in declaration order, no extras, no whitespace, plain integers.
//! Rather than a bespoke strict parser, [`parse`] decodes permissively,
//! re-encodes canonically, and requires the result to equal the input byte for
//! byte — reordered keys, added whitespace, `1.0` for `1` all differ after the
//! round trip and are refused. A cursor a ZIG daemon issued is canonical under
//! the same rule, so it survives the cutover.
//!
//! # One generation number, not one per payload
//!
//! `http/pagination.zig` owns a single version across every struct cursor it
//! issues, and [`VERSION`] is that number. A payload reports the generation it
//! was decoded under through [`StructCursor::generation`] rather than declaring
//! a constant of its own, so two payloads cannot drift onto different numbers
//! and leave a reader guessing which one a token belongs to.
//!
//! # Two refusals, deliberately distinct
//!
//! [`Foreign::Malformed`] is "not a cursor this endpoint issued";
//! [`Foreign::VersionMismatch`] is "issued by a different payload generation".
//! Callers answer both with `UZ-LIBRARY-001` — the split exists so a
//! deploy-boundary spike is legible in logs rather than hiding inside a generic
//! parse failure. Identity mismatch (`UZ-LIBRARY-002`) is NOT here: only the
//! handler knows the active filters, tenant and limit, so only the handler can
//! compare them.

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as BASE64;
use serde::Serialize;
use serde::de::DeserializeOwned;

/// The generation every struct cursor this daemon issues is stamped with.
///
/// A decoded payload carrying any other value is refused, which is what stops a
/// deploy from silently reinterpreting yesterday's boundary.
pub const VERSION: u8 = 2;

/// A page boundary that travels as an opaque token.
///
/// Implemented by the payload struct itself. `#[serde(deny_unknown_fields)]` on
/// the implementor is what makes an added key a refusal rather than a silently
/// ignored one, and field ORDER is the canonical key order — reordering the
/// declaration invalidates every token already in flight.
pub trait StructCursor: Serialize + DeserializeOwned {
    /// The generation this payload was stamped with.
    ///
    /// Read off the decoded value rather than from the type, because the
    /// question [`parse`] asks is what the TOKEN claims, not what this build
    /// would have written.
    fn generation(&self) -> u8;
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
/// Infallible, and not by swallowing anything: `serde_json` fails on a map with
/// non-string keys or a type whose `Serialize` raises, and this trait is for
/// structs of strings and integers. Returning a `Result` would put an error arm
/// on every caller for a branch none of them can reach, and `expect` is denied
/// workspace-wide because these crates link into a daemon — so the unreachable
/// half is spelled as the empty string and the next `parse` refuses it.
#[must_use]
pub fn render<C: StructCursor>(cursor: &C) -> String {
    let json = serde_json::to_string(cursor).unwrap_or_default();
    BASE64.encode(json)
}

/// Reads a token this daemon — or the Zig one — issued.
///
/// # Errors
/// Refuses anything that is not unpadded base64url of the canonical JSON form,
/// and a well-formed payload from another generation — distinctly, for the
/// module note's reason.
pub fn parse<C: StructCursor>(token: &str) -> Result<C, Foreign> {
    let json = BASE64
        .decode(token)
        .map_err(|_undecodable| Foreign::Malformed)?;
    let cursor: C = serde_json::from_slice(&json).map_err(|_not_the_shape| Foreign::Malformed)?;
    // Canonicity: re-encode and require identical bytes. Reordered keys, extra
    // whitespace, a float spelling of an integer — each survives the permissive
    // parse above and each fails here. A re-encode that itself failed cannot
    // match either, so the fallback is the refusal rather than a second panic
    // path on a value a caller supplied.
    let canonical = serde_json::to_vec(&cursor).unwrap_or_default();
    if canonical != json {
        return Err(Foreign::Malformed);
    }
    if cursor.generation() != VERSION {
        return Err(Foreign::VersionMismatch);
    }
    Ok(cursor)
}

#[cfg(test)]
mod tests {
    use super::{Foreign, StructCursor, VERSION, parse, render};
    use serde::{Deserialize, Serialize};

    /// A two-field payload standing in for a real page's boundary.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Boundary {
        v: u8,
        id: String,
        limit: u32,
    }

    impl StructCursor for Boundary {
        fn generation(&self) -> u8 {
            self.v
        }
    }

    fn sample() -> Boundary {
        Boundary {
            v: VERSION,
            id: "0195b4ba-8d3a-7f13-8abc-cd0000000002".to_owned(),
            limit: 50,
        }
    }

    #[test]
    fn a_rendered_token_round_trips() {
        let token = render(&sample());
        assert_eq!(parse::<Boundary>(&token), Ok(sample()));
    }

    #[test]
    fn the_wire_form_is_unpadded_base64url_of_declaration_order_json() {
        // Pinned bytes: what `std.json.Stringify` emits for this payload,
        // base64url without padding. A client holding a Zig-issued cursor must
        // be able to spend it here mid-cutover, so the encoding is a wire fact
        // rather than a choice.
        use base64::Engine as _;
        let json = "{\"v\":2,\"id\":\"0195b4ba-8d3a-7f13-8abc-cd0000000002\",\"limit\":50}";
        let token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(json);
        assert_eq!(render(&sample()), token);
        assert_eq!(parse::<Boundary>(&token), Ok(sample()));
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
            encode("{\"id\":\"i\",\"v\":2,\"limit\":50}"),
            encode("{\"v\":2,\"id\":\"i\",\"limit\":50,\"extra\":true}"),
            encode("{\"v\": 2,\"id\":\"i\",\"limit\":50}"),
            encode("{\"v\":2,\"id\":\"i\",\"limit\":50.0}"),
        ] {
            assert_eq!(
                parse::<Boundary>(&bad),
                Err(Foreign::Malformed),
                "{bad:?} is not a cursor this daemon issued"
            );
        }
    }

    #[test]
    fn another_generation_is_refused_distinctly() {
        let old = Boundary { v: 1, ..sample() };
        let token = render(&old);
        assert_eq!(parse::<Boundary>(&token), Err(Foreign::VersionMismatch));
    }
}
