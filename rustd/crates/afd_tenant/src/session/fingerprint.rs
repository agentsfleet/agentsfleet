//! Who asked, as a digest that identifies a repeat without identifying a person.
//!
//! # What it is for
//!
//! A device-flow redemption is answered exactly once, and then repeated inside
//! a sixty-second window for whoever asked FIRST — because a command line whose
//! reply was lost to a dropped connection has to be able to ask again, and a
//! session it can never re-read is a login that fails for a network hiccup. The
//! window is safe only if "whoever asked first" is checkable, which is what
//! this digest is.
//!
//! # The separator is load-bearing
//!
//! Concatenating the three fields and hashing the result makes the field
//! boundaries invisible: `("12", "3")` and `("1", "23")` hash identically, so a
//! caller who controls the user agent can craft one that collides with somebody
//! else's address inside the replay window. A zero byte cannot appear in either
//! an address's textual form or an HTTP header value, per RFC 9110's field-value
//! grammar, so it
//! cannot be smuggled in from either field.

use sha2::{Digest as _, Sha256};

/// The byte that separates two fields, for the reason the module note gives.
const FIELD_SEPARATOR: u8 = 0x00;

/// A request's identity for the replay window: address, user agent, session.
///
/// Holds the rendered hex rather than the digest bytes, because the only thing
/// done with it is comparing it against what a Lua script stored as text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fingerprint(String);

impl Fingerprint {
    /// Digests one request's identity.
    #[must_use]
    pub fn of(client_address: &str, user_agent: &str, session_id: &str) -> Self {
        let mut hasher = Sha256::new();
        for field in [client_address, user_agent] {
            hasher.update(field.as_bytes());
            hasher.update([FIELD_SEPARATOR]);
        }
        hasher.update(session_id.as_bytes());
        Self(hex::encode(hasher.finalize()))
    }

    /// The lower-case hex the script compares.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_triple_always_digests_the_same_way() {
        let first = Fingerprint::of("203.0.113.7", "afc/1", "sid");
        let second = Fingerprint::of("203.0.113.7", "afc/1", "sid");
        assert_eq!(first, second);
        assert_eq!(first.as_str().len(), 64);
    }

    #[test]
    fn changing_any_field_changes_the_digest() {
        let base = Fingerprint::of("203.0.113.7", "afc/1", "sid");
        for other in [
            Fingerprint::of("203.0.113.8", "afc/1", "sid"),
            Fingerprint::of("203.0.113.7", "afc/2", "sid"),
            Fingerprint::of("203.0.113.7", "afc/1", "sid2"),
        ] {
            assert_ne!(base, other);
        }
    }

    #[test]
    fn the_separator_keeps_the_field_boundary_visible() {
        // Without it these two hash the same bytes, and a crafted user agent
        // could then claim another caller's replay window.
        assert_ne!(
            Fingerprint::of("12", "3", "sid"),
            Fingerprint::of("1", "23", "sid")
        );
    }
}
