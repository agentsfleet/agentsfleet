//! A resolved provider owns its redacted, zeroizing credential.

use afd_billing::Posture;
use afd_crypto::secret::SecretString;

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
    use afd_billing::Posture;

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
