//! What the vault holds under a name, decided without opening it.
//!
//! # Two answers, and the third is somebody else's
//!
//! The write path's ladder asks two separate questions before it will store a
//! self-managed selection: is there a credential under this name at all, and is
//! what is there a PROVIDER key. They are different refusals with different
//! repairs — "store that credential first" against "that credential is not a
//! provider key" — and this type answers only the SECOND. Absence is decided
//! before it: the locked read either returns a row or does not, so a variant
//! for "no row" here could never be constructed, and a reader matching on one
//! would look for a path that does not exist.
//!
//! # Nothing is decrypted to answer either
//!
//! `vault.secrets` carries `meta_provider` and `meta_has_key` beside the
//! ciphertext precisely so a caller can ask what KIND of credential a row holds
//! without holding it. Reading the metadata instead of decrypting means the
//! refusal path never has a plaintext key in memory at all — one fewer place a
//! key exists, on the path most likely to be walked by a client getting it
//! wrong. The activation transaction asks this
//! of the meta columns on the vault row it has already locked, so the answer
//! costs no extra statement.

/// What the vault holds under a name, as far as the metadata can say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretKind {
    /// A row exists, and its metadata does not describe a provider key.
    NotAProviderKey,
    /// A row exists and describes a provider key.
    ProviderKey,
}

impl SecretKind {
    /// Reads the vault's two metadata columns into one answer.
    ///
    /// A provider key names a provider and carries a key. The compatible
    /// provider is no exception: resolution reads the key and the catalogue
    /// entry for every provider alike, so a credential without a key could be
    /// activated and then fail every run it was chosen for.
    ///
    /// Reads a row that EXISTS; the store answers absence before calling it.
    #[must_use]
    pub fn of(provider: Option<&str>, has_key: Option<bool>) -> Self {
        match (provider, has_key) {
            (Some(_), Some(true)) => Self::ProviderKey,
            // A named provider with no key, and a row naming no provider at
            // all — an env var, a webhook secret — are one answer to a caller:
            // the row is there and it is not a provider key.
            _not_dialable => Self::NotAProviderKey,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SecretKind;

    /// The compatible provider, which resolution treats like any other.
    const COMPATIBLE: &str = "openai-compatible";

    /// A provider whose credential is worthless without a bearer key.
    const NAMED: &str = "anthropic";

    /// A vault row holding something that is not a provider credential.
    const NOT_A_PROVIDER: Option<&str> = None;

    #[test]
    fn a_named_provider_with_a_key_is_a_provider_key() {
        assert_eq!(
            SecretKind::of(Some(NAMED), Some(true)),
            SecretKind::ProviderKey
        );
    }

    #[test]
    fn a_named_provider_without_a_key_is_not_a_provider_key() {
        // The ladder's third rung. The row EXISTS — the second rung's question,
        // already answered — and what is in it cannot be dialled.
        assert_eq!(
            SecretKind::of(Some(NAMED), Some(false)),
            SecretKind::NotAProviderKey
        );
        assert_eq!(
            SecretKind::of(Some(NAMED), None),
            SecretKind::NotAProviderKey
        );
    }

    /// Resolution reads the key for every provider, so a compatible endpoint
    /// without one cannot run a model and is not a provider key either.
    #[test]
    fn a_compatible_credential_needs_a_key_like_any_other() {
        for has_key in [Some(false), None] {
            assert_eq!(
                SecretKind::of(Some(COMPATIBLE), has_key),
                SecretKind::NotAProviderKey
            );
        }
        assert_eq!(
            SecretKind::of(Some(COMPATIBLE), Some(true)),
            SecretKind::ProviderKey
        );
    }

    #[test]
    fn a_row_naming_no_provider_is_not_a_provider_key() {
        assert_eq!(
            SecretKind::of(NOT_A_PROVIDER, Some(true)),
            SecretKind::NotAProviderKey
        );
    }
}
