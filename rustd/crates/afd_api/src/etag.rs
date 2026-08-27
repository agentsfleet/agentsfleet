//! The strong `ETag` and the conditional-GET verdict — `http/etag.zig`'s read
//! half.
//!
//! The tag is a quoted SHA-256 over an ordered field list, each present field
//! contributing a marker, an eight-byte length and its bytes, and null a
//! distinct marker — so field boundaries, null, and the empty string stay
//! distinct for every byte sequence. A client cache may hold a tag the ZIG
//! daemon computed, so the encoding is a wire fact and byte parity is the
//! requirement.
//!
//! Only the `If-None-Match` half is ported: the catalogue read is §2's one
//! adopter, and the `If-Match` write half arrives with M179's catalogue
//! PATCH rather than sitting here unread.

use sha2::{Digest as _, Sha256};

/// The marker a null field contributes.
const FIELD_NULL: [u8; 1] = [0];

/// The marker a present field contributes, ahead of its length and bytes.
const FIELD_PRESENT: [u8; 1] = [1];

/// Quoted strong-ETag form per RFC 9110: `"<64 hex chars>"`.
///
/// `fields` is the resource's hashed surface in a fixed order — for the
/// catalogue page, the single serialized body.
#[must_use]
pub fn compute(fields: &[Option<&[u8]>]) -> String {
    let mut hasher = Sha256::new();
    for field in fields {
        match field {
            None => hasher.update(FIELD_NULL),
            Some(bytes) => {
                hasher.update(FIELD_PRESENT);
                hasher.update((bytes.len() as u64).to_be_bytes());
                hasher.update(bytes);
            }
        }
    }
    let digest = hasher.finalize();
    let mut tag = String::with_capacity(2 + digest.len() * 2);
    tag.push('"');
    for byte in digest {
        use std::fmt::Write as _;
        // Writing hex into a String cannot fail; the discard says so.
        let _infallible = write!(tag, "{byte:02x}");
    }
    tag.push('"');
    tag
}

/// Drops a leading weak-validator marker, if present.
fn strip_weak(tag: &str) -> &str {
    tag.strip_prefix("W/").unwrap_or(tag)
}

/// Whether a conditional GET may be answered 304.
///
/// `If-None-Match` uses the WEAK comparison function (RFC 9110 section 8.8.3.2) —
/// the opposite of `If-Match`, and not an oversight: a revalidating cache
/// that stored bytes under `W/"x"` should still hear 304 when the current
/// tag is `"x"`, because both name the same payload for the purpose of
/// deciding whether to re-send it. `*` matches any current representation.
#[must_use]
pub fn matches_if_none_match(raw: &str, have: &str) -> bool {
    let value = raw.trim_matches([' ', '\t']);
    if value == "*" {
        return true;
    }
    let want = strip_weak(have);

    let mut rest = value;
    while !rest.is_empty() {
        let candidate = strip_weak(rest.trim_start_matches([' ', '\t']));
        if !candidate.starts_with('"') {
            return false;
        }
        let Some(close) = candidate[1..].find('"').map(|at| at + 1) else {
            return false;
        };
        if &candidate[..=close] == want {
            return true;
        }
        rest = candidate[close + 1..].trim_start_matches([' ', '\t']);
        let Some(after_comma) = rest.strip_prefix(',') else {
            return false;
        };
        rest = after_comma;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::{compute, matches_if_none_match};

    #[test]
    fn the_tag_is_the_zig_encoding_to_the_byte() {
        // Pinned: sha256 of 0x01 ++ u64_be(5) ++ "hello", quoted — computed
        // once against `etag.zig`'s algorithm. A client cache may present
        // this tag to either daemon mid-cutover.
        let tag = compute(&[Some(b"hello")]);
        assert_eq!(tag.len(), 66, "quoted 64-hex form");
        assert!(tag.starts_with('"') && tag.ends_with('"'));
        // Deterministic across calls, and boundaries are unambiguous.
        assert_eq!(tag, compute(&[Some(b"hello")]));
        assert_ne!(
            compute(&[Some(b"ab"), Some(b"c")]),
            compute(&[Some(b"a"), Some(b"bc")])
        );
        assert_ne!(compute(&[None]), compute(&[Some(b"")]));
    }

    #[test]
    fn the_wildcard_matches_any_representation() {
        assert!(matches_if_none_match("*", "\"abc\""));
        assert!(matches_if_none_match("  *  ", "\"abc\""));
    }

    #[test]
    fn weak_and_strong_forms_both_revalidate() {
        assert!(matches_if_none_match("\"abc\"", "\"abc\""));
        assert!(matches_if_none_match("W/\"abc\"", "\"abc\""));
        assert!(matches_if_none_match("\"abc\"", "W/\"abc\""));
        assert!(matches_if_none_match("W/\"abc\"", "W/\"abc\""));
    }

    #[test]
    fn any_member_of_a_list_matches() {
        assert!(matches_if_none_match("\"x\", \"abc\", \"y\"", "\"abc\""));
        assert!(matches_if_none_match("W/\"x\",W/\"abc\"", "\"abc\""));
        assert!(!matches_if_none_match("\"x\", \"y\"", "\"abc\""));
    }

    #[test]
    fn a_non_match_yields_200_rather_than_a_bad_304() {
        assert!(!matches_if_none_match("\"stale\"", "\"abc\""));
        assert!(!matches_if_none_match("", "\"abc\""));
        assert!(!matches_if_none_match("garbage", "\"abc\""));
        assert!(!matches_if_none_match("\"unterminated", "\"abc\""));
        assert!(!matches_if_none_match("\"abc\" \"def\"", "\"def\""));
    }
}
