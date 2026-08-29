//! The shared conditional-GET verdict — `http/etag.zig`'s comparison half.
//!
//! The DIGEST moved to [`afd_core::etag`] when a second caller appeared:
//! `afd_fleet_lifecycle` compares a tag inside the row lock a conditional write
//! holds, where this crate cannot reach, and two spellings of one wire encoding
//! fail silently — a client's cached tag simply stops matching. It is
//! re-exported here so a handler still names `crate::etag::compute`.
//!
//! What stays is the comparison, which is an HTTP rule rather than an encoding:
//! `If-None-Match` uses the WEAK function, list membership, and `*`.

pub use afd_core::etag::compute;

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
    use super::matches_if_none_match;

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
