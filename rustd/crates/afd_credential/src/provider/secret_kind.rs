//! What the vault holds under a name, decided without opening it.
//!
//! # Three answers, because there are three repairs
//!
//! The write path's ladder asks two separate questions before it will store a
//! self-managed selection: is there a credential under this name at all, and is
//! what is there a PROVIDER key. They are different refusals with different
//! repairs — "store that credential first" against "that credential is not a
//! provider key" — so a `bool` here would collapse them and leave the caller
//! guessing which sentence to serve.
//!
//! # Nothing is decrypted to answer either
//!
//! `vault.secrets` carries `meta_provider` and `meta_has_key` beside the
//! ciphertext precisely so a caller can ask what KIND of credential a row holds
//! without holding it. `tenant_provider.zig` decrypts to answer this; reading
//! the metadata instead means the refusal path never has a plaintext key in
//! memory at all — one fewer place a key exists, on the path most likely to be
//! walked by a client getting it wrong.

use afd_core::id::Uuid7;
use sqlx::Row as _;

use crate::error::{Result, query};
use crate::provider::endpoint::OPENAI_COMPATIBLE;
use crate::provider::sql;
use crate::provider::store::Providers;

/// Statement name, for the context a query failure carries.
const CONTEXT_SECRET_SHAPE: &str = "vault credential shape";

/// What the vault holds under a name, as far as the metadata can say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretKind {
    /// No row in this workspace carries the name.
    Absent,
    /// A row exists, and its metadata does not describe a provider key.
    NotAProviderKey,
    /// A row exists and describes a provider key.
    ProviderKey,
}

impl SecretKind {
    /// Reads the vault's two metadata columns into one answer.
    ///
    /// A named provider must carry a key; `openai-compatible` need not, because
    /// its endpoint may be unauthenticated and a tenant fronting their own
    /// gateway has no bearer token to give. That asymmetry is the registry
    /// hint's own wording, and it is why `has_key` alone does not decide this.
    ///
    /// Never answers [`Self::Absent`]: this reads a row that EXISTS, and the
    /// store answers absence before calling it.
    #[must_use]
    pub fn of(provider: Option<&str>, has_key: Option<bool>) -> Self {
        match (provider, has_key) {
            // One arm rather than two, because the two are the same answer: a
            // compatible endpoint is dialable with or without a key, and every
            // other named provider is dialable only with one.
            (Some(OPENAI_COMPATIBLE), _) | (Some(_), Some(true)) => Self::ProviderKey,
            // A named provider with no key, and a row naming no provider at
            // all — an env var, a webhook secret — are one answer to a caller:
            // the row is there and it is not a provider key.
            _not_dialable => Self::NotAProviderKey,
        }
    }
}

impl Providers {
    /// What kind of credential `workspace` holds under `name`.
    ///
    /// One round trip decides both of the write ladder's rungs, off the
    /// non-secret metadata — see
    /// [`SELECT_SECRET_SHAPE`](crate::provider::sql::SELECT_SECRET_SHAPE).
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub async fn secret_kind(&self, workspace: &Uuid7, name: &str) -> Result<SecretKind> {
        let mut connection = self.pool().acquire().await?;
        let found = sqlx::query(sql::SELECT_SECRET_SHAPE)
            .bind(workspace.as_str())
            .bind(name)
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_SECRET_SHAPE))?;

        let Some(row) = found else {
            return Ok(SecretKind::Absent);
        };
        let unreadable = query(CONTEXT_SECRET_SHAPE);
        let provider: Option<String> = row.try_get(0).map_err(&unreadable)?;
        let has_key: Option<bool> = row.try_get(1).map_err(&unreadable)?;

        Ok(SecretKind::of(provider.as_deref(), has_key))
    }
}

#[cfg(test)]
mod tests {
    use super::{OPENAI_COMPATIBLE, SecretKind};

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

    #[test]
    fn a_compatible_credential_needs_no_key() {
        // The one asymmetry, and the registry hint says so: a custom endpoint
        // may be unauthenticated, so an absent key is not an absent credential.
        for has_key in [Some(false), None] {
            assert_eq!(
                SecretKind::of(Some(OPENAI_COMPATIBLE), has_key),
                SecretKind::ProviderKey
            );
        }
    }

    #[test]
    fn a_row_naming_no_provider_is_not_a_provider_key() {
        assert_eq!(
            SecretKind::of(NOT_A_PROVIDER, Some(true)),
            SecretKind::NotAProviderKey
        );
    }

    #[test]
    fn absence_is_reachable_only_from_a_missing_row() {
        // `of` reads a row that exists, so it must never answer Absent —
        // conflating the two would turn "no such credential" into "that
        // credential is wrong", which is a different repair.
        for provider in [NOT_A_PROVIDER, Some(NAMED), Some(OPENAI_COMPATIBLE)] {
            for has_key in [None, Some(true), Some(false)] {
                assert_ne!(SecretKind::of(provider, has_key), SecretKind::Absent);
            }
        }
    }
}
