//! What resolution answers with, and the one field that must not escape it.
//!
//! # The key is private, and that is Invariant 3
//!
//! The milestone says the provider `api_key` never enters `secrets_map` and is
//! zeroed after the lease is serialised. `ResolvedProvider` states that as a
//! doc comment over a `pub api_key: []u8` and a `deinit` the caller must
//! remember to call — so the invariant holds for exactly as long as every
//! future call site remembers two things.
//!
//! Here the field is private and there is no `Deref`, no getter returning an
//! owned copy, and no infallible conversion out. The only way to read the key
//! is [`SecretString::expose`], which borrows — so a caller cannot obtain a
//! copy this type is no longer able to zero, and the map-building code in the
//! sibling slice cannot reach it by accident, because it cannot reach it at
//! all. That is `M-STRONG-TYPES-GUARD` pointed at a security invariant, the
//! same move [`afd_crypto::secret::SecretBytes`] makes one layer down.
//!
//! # Zeroing is a destructor, not a `defer`
//!
//! The Zig threads a `committed` flag and a `defer if (!committed) …deinit()`
//! through the whole billing pass so the key is wiped on every early return
//! and NOT wiped on the one path that hands it onward. That is a hand-rolled
//! move, and the flag is what a move already means. Dropping the flag is not a
//! tidy-up: it deletes the failure mode where a new early `return` is added
//! above the `defer` and the key survives it.

use std::fmt::{self, Debug, Formatter};

use serde::de::{Deserialize, Deserializer};
use zeroize::Zeroizing;

use crate::money::Posture;

/// A string that is wiped when it goes out of scope, however it got there.
///
/// Deserialised IN PLACE rather than built from a `String` at the call site:
/// `String::deserialize` produces one heap buffer, and wrapping it here MOVES
/// that buffer rather than copying it, so there is never a second allocation
/// holding the same bytes with no destructor. A `#[derive(Deserialize)]` field
/// typed `String` and converted afterwards would leave exactly that copy behind
/// on every error path between the two.
///
/// Deliberately NOT `PartialEq`. Comparing key material with `==` is a
/// byte-by-byte walk that stops at the first difference, which leaks how much
/// of a guess was right through how long it took to reject (RULE CTM). Nothing
/// in this crate needs to compare two provider keys, so the operation is absent
/// rather than provided in a form that would be wrong to use.
#[derive(Clone)]
pub struct SecretString(Zeroizing<String>);

impl SecretString {
    /// Wraps a string so it is wiped when dropped.
    #[must_use]
    pub fn new(value: String) -> Self {
        Self(Zeroizing::new(value))
    }

    /// The characters, borrowed for as long as this value lives.
    ///
    /// Borrowed and not moved: handing back an owned `String` would create a
    /// copy this type can no longer wipe, which is the whole thing it exists to
    /// prevent.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }

    /// Whether it carries nothing.
    ///
    /// The check both credential shapes run — an empty key is a malformed
    /// credential for every provider that needs one — asked without exposing
    /// the value to ask it.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Reads a JSON string straight into its wiping wrapper.
///
/// Hand-written rather than derived: `Zeroizing` does not implement
/// `Deserialize` in the feature set this workspace takes it under, and a
/// `#[serde(transparent)]` over a field that cannot deserialise would not
/// compile. Six lines, and they are the six that make the MOVE explicit — the
/// buffer `String::deserialize` allocated is the buffer that gets wiped, with
/// no copy in between.
impl<'de> Deserialize<'de> for SecretString {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer)
            .map(Zeroizing::new)
            .map(Self)
    }
}

/// Renders a placeholder, never the characters.
///
/// `M-PUBLIC-DEBUG`: a sensitive type still implements `Debug`, through an
/// implementation whose redaction has a test behind it. Without one, the
/// derived `Debug` on every struct that CONTAINS this would print the key —
/// and [`Resolved`] is carried through the whole admission pass.
impl Debug for SecretString {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("SecretString(redacted)")
    }
}

/// One tenant's provider, resolved for one lease.
///
/// Owned rather than borrowed: it outlives the reads that produced it and is
/// carried into the lease row and the execution policy. The provider named here
/// is the provider that will be BILLED, because there is no second resolution
/// to disagree with it — which is what makes "the key we billed is the key we
/// deliver" structural rather than a comment.
///
/// A custom endpoint, and the host the egress allowlist admits for it.
///
/// The two are one value because they are one decision. A shape carrying only
/// the URL leaves every consumer re-deriving the host, and a second derivation
/// is a second chance to disagree with the one that made the SSRF ruling — so
/// the run could dial a URL whose host the allowlist never actually cleared.
/// [`super::endpoint::validate`] produces both at once; this keeps them
/// together from there to the wire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dialled {
    /// The URL the run dials.
    pub base_url: Box<str>,
    /// The bare host, as the egress allowlist spells it.
    pub inference_host: Box<str>,
}

/// Not `PartialEq`, because [`SecretString`] is not — see that type for why.
#[derive(Debug, Clone)]
pub struct Resolved {
    /// Who supplies the key, and therefore who pays for tokens.
    pub posture: Posture,
    /// The provider the run dials.
    pub provider: Box<str>,
    /// The model it is priced against.
    pub model: Box<str>,
    /// The context ceiling the engine is handed.
    pub context_cap_tokens: u32,
    /// A validated custom endpoint, or `None` for a named provider dialing a
    /// built-in host.
    ///
    /// Non-`None` only after [`super::endpoint::validate`] accepted it, so a
    /// value here is already https and already SSRF-safe — interior code needs
    /// no defensive re-check, and there is none.
    pub endpoint: Option<Dialled>,
    /// The key itself.
    ///
    /// Private. See the module note: this is the field Invariant 3 is about,
    /// and the only way to it is [`Resolved::api_key`].
    api_key: SecretString,
}

impl Resolved {
    /// Assembles a resolution around its key.
    ///
    /// Takes the key LAST and by value, so the only way to build one is to give
    /// up ownership of the string — there is no constructor that borrows it and
    /// leaves the caller holding a copy.
    #[must_use]
    pub const fn new(
        posture: Posture,
        provider: Box<str>,
        model: Box<str>,
        context_cap_tokens: u32,
        endpoint: Option<Dialled>,
        api_key: SecretString,
    ) -> Self {
        Self {
            posture,
            provider,
            model,
            context_cap_tokens,
            endpoint,
            api_key,
        }
    }

    /// The provider key, borrowed.
    #[must_use]
    pub const fn api_key(&self) -> &SecretString {
        &self.api_key
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        clippy::unwrap_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]
    use super::{Resolved, SecretString};
    use crate::money::Posture;

    fn resolved() -> Resolved {
        Resolved::new(
            Posture::Platform,
            "anthropic".into(),
            "claude-opus-5".into(),
            200_000,
            None,
            SecretString::new("sk-ant-not-a-real-key".to_owned()),
        )
    }

    #[test]
    fn a_resolution_never_renders_its_key() {
        // The failure this prevents is not hypothetical: `Resolved` is carried
        // on the admission pass, and one `tracing` field spelled `?resolved`
        // would put a live provider key in the log stream of every lease.
        let rendered = format!("{:?}", resolved());
        assert!(
            !rendered.contains("sk-ant"),
            "a resolution rendered its key: {rendered}"
        );
        assert!(rendered.contains("SecretString(redacted)"));
        // And the model and provider DO render, because an operator reading a
        // lease line needs them and neither is sensitive.
        assert!(rendered.contains("claude-opus-5"));
    }

    #[test]
    fn the_key_is_reachable_only_by_borrowing_it() {
        let resolved = resolved();
        assert_eq!(resolved.api_key().expose(), "sk-ant-not-a-real-key");
        assert!(!resolved.api_key().is_empty());
        assert!(SecretString::new(String::new()).is_empty());
    }

    #[test]
    fn a_key_deserialises_straight_into_its_wrapper() {
        // `serde(transparent)`, so a credential field typed `SecretString`
        // reads exactly as a string field would — there is no wrapper object
        // in the stored JSON to keep in step.
        let parsed: SecretString =
            serde_json::from_str("\"sk-live-value\"").expect("a JSON string is a secret string");
        assert_eq!(parsed.expose(), "sk-live-value");
        serde_json::from_str::<SecretString>("42").unwrap_err();
    }
}
