//! Whether a presented `If-Match` admits the tag an entry carries now.
//!
//! The tag itself is `afd_core::etag::compute`, the workspace's one encoding:
//! a client holds it across deployments and presents it back.

use headers::{ETag, Header as _, IfMatch};

pub(super) fn matches_if_match(raw: &str, current: &str) -> bool {
    let Ok(value) = raw.parse() else {
        return false;
    };
    let Ok(condition) = IfMatch::decode(&mut core::iter::once(&value)) else {
        return false;
    };
    current
        .parse::<ETag>()
        .is_ok_and(|etag| condition.precondition_passes(&etag))
}

#[cfg(test)]
mod tests {
    use super::*;

    use afd_core::etag::compute;

    /// The tag an entry answered with before this crate used the shared
    /// encoding, so a client's cached tag still matches after a deploy.
    #[test]
    fn tag_is_quoted_deterministic_and_field_bounded() {
        let first = compute(&[Some(b"ab"), Some(b"c"), None]);
        // pin test: literal is the contract
        assert_eq!(
            first,
            "\"6d7eab446fb051dbd0811a427998aea9f4edacf43cc0a8747cec4991d9d13842\""
        );
        assert_ne!(first, compute(&[Some(b"a"), Some(b"bc"), None]));
        assert_ne!(first, compute(&[Some(b"ab"), Some(b"c"), Some(b"")]));
    }

    #[test]
    fn if_match_uses_strong_comparison_and_lists() {
        assert!(matches_if_match("*", "\"abc\""));
        assert!(matches_if_match("\"x\", \"abc\"", "\"abc\""));
        assert!(!matches_if_match("W/\"abc\"", "\"abc\""));
        assert!(!matches_if_match("garbage", "\"abc\""));
    }
}
