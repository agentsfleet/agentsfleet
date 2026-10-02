//! The name a Fleet Bundle is stored and fetched under.
//!
//! The importer names a bundle by this digest and the runner verifies the bytes
//! it fetches against it, so the two must compute it identically — which is
//! why there is one implementation and both call it. It is not a hash of the
//! archive: it covers the instructions, the trigger and each support file's
//! path and content, so the same bundle packed twice keeps one name.

use sha2::{Digest as _, Sha256};

/// Ends every part, so `ab` + `c` and `a` + `bc` never hash alike.
const SEPARATOR: [u8; 1] = [0];

/// A bundle's content digest, built part by part in the order the importer
/// stores them.
#[derive(Debug, Clone)]
pub struct BundleDigest(Sha256);

impl BundleDigest {
    /// Starts with the instructions and the trigger, which is empty when the
    /// bundle carries none.
    #[must_use]
    pub fn new(skill: &[u8], trigger: Option<&[u8]>) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(skill);
        hasher.update(SEPARATOR);
        hasher.update(trigger.unwrap_or_default());
        hasher.update(SEPARATOR);
        Self(hasher)
    }

    /// Adds one support file, in the bundle's stored order.
    pub fn support_file(&mut self, path: &str, content: &[u8]) {
        self.0.update(path.as_bytes());
        self.0.update(SEPARATOR);
        self.0.update(content);
        self.0.update(SEPARATOR);
    }

    /// The digest, as the lowercase hex the bundle is named by.
    #[must_use]
    pub fn finish(self) -> String {
        hex::encode(self.0.finalize())
    }
}

#[cfg(test)]
mod tests {
    use super::BundleDigest;

    /// A bundle with no trigger and no support files, pinned so a change to
    /// the part order cannot rename every stored bundle unnoticed.
    #[test]
    fn a_skill_only_bundle_has_a_fixed_name() {
        let named = BundleDigest::new(b"skill", None).finish();

        // pin test: literal is the contract
        assert_eq!(named, "0b8f5cc070407bc630e301f32d852d26a955a5a1b16d3a257c57fe414346b349");
    }

    /// Moving a byte across a part boundary changes the name.
    #[test]
    fn parts_cannot_trade_bytes() {
        let mut left = BundleDigest::new(b"s", Some(b"t"));
        left.support_file("ab", b"c");
        let mut right = BundleDigest::new(b"s", Some(b"t"));
        right.support_file("a", b"bc");

        assert_ne!(left.finish(), right.finish());
    }

    /// An absent trigger and an empty one name the same bundle.
    #[test]
    fn an_absent_trigger_is_an_empty_one() {
        assert_eq!(
            BundleDigest::new(b"s", None).finish(),
            BundleDigest::new(b"s", Some(b"")).finish()
        );
    }
}
