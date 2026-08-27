//! Strong entity tags over one ordered editable surface.

use headers::{ETag, Header as _, IfMatch};
use sha2::{Digest as _, Sha256};

const FIELD_NULL: u8 = 0;
const FIELD_PRESENT: u8 = 1;

pub(super) fn compute(fields: &[Option<&str>]) -> String {
    let mut digest = Sha256::new();
    for field in fields {
        match field {
            Some(value) => {
                digest.update([FIELD_PRESENT]);
                digest.update(u64::try_from(value.len()).unwrap_or(u64::MAX).to_be_bytes());
                digest.update(value.as_bytes());
            }
            None => digest.update([FIELD_NULL]),
        }
    }
    format!("\"{}\"", hex::encode(digest.finalize()))
}

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

    #[test]
    fn tag_is_quoted_deterministic_and_field_bounded() {
        let first = compute(&[Some("ab"), Some("c"), None]);
        assert_eq!(first, compute(&[Some("ab"), Some("c"), None]));
        assert_ne!(first, compute(&[Some("a"), Some("bc"), None]));
        assert_ne!(first, compute(&[Some("ab"), Some("c"), Some("")]));
        assert_eq!(first.len(), 66);
        assert!(first.starts_with('"') && first.ends_with('"'));
    }

    #[test]
    fn if_match_uses_strong_comparison_and_lists() {
        assert!(matches_if_match("*", "\"abc\""));
        assert!(matches_if_match("\"x\", \"abc\"", "\"abc\""));
        assert!(!matches_if_match("W/\"abc\"", "\"abc\""));
        assert!(!matches_if_match("garbage", "\"abc\""));
    }
}
