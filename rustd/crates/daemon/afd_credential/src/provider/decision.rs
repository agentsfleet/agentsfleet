//! Decision calls spend the deployment's own key, read again for each dispatch.

use afd_core::{id::Uuid7, json::Unreadable};
use afd_crypto::secret::SecretString;
use afd_wire::decision::DecisionApi;
use serde::Deserialize;

use super::Providers;
use crate::{Result, error::ErrorKind, vault::KeyRef};

/// The one field a decision call spends; unrelated fields stay in the vault.
/// A typed secret guards the parsed allocation without building a JSON tree.
#[derive(Debug, Deserialize)]
struct StoredKey {
    api_key: SecretString,
}

impl Providers {
    /// Opens the platform-admin key for one already-reserved physical call.
    ///
    /// `admin` comes from the daemon's platform-admin workspace setting, never
    /// from the lease, model configuration or tenant. No cached default or
    /// environment key can substitute for a missing or revoked row.
    ///
    /// # Errors
    /// Reports an absent/unusable key, a datastore refusal or an unopened
    /// envelope. Malformed plaintext is reduced to value-free diagnostics.
    #[tracing::instrument(name = "decision_credential_resolve", skip_all, fields(decision_api = ?api))]
    pub async fn resolve_decision_key(
        &self,
        admin: &Uuid7,
        api: DecisionApi,
    ) -> Result<SecretString> {
        let body = self
            .vault()
            .open(KeyRef {
                workspace_id: admin,
                name: bag(api),
            })
            .await?
            .ok_or(ErrorKind::DecisionCredentialMissing { api })?;
        decode(body.expose(), api)
    }

    /// Fresh resolution on a caller's bounded SQL session, without lifecycle locks.
    ///
    /// # Errors
    /// Preserves the vault, absence and typed parsing failures of the owned read.
    #[tracing::instrument(name = "decision_credential_resolve", skip_all, fields(decision_api = ?api))]
    pub async fn resolve_decision_key_on(
        &self,
        connection: &mut sqlx::PgConnection,
        admin: &Uuid7,
        api: DecisionApi,
    ) -> Result<SecretString> {
        let body = self
            .vault()
            .open_on(
                connection,
                KeyRef {
                    workspace_id: admin,
                    name: bag(api),
                },
            )
            .await?
            .ok_or(ErrorKind::DecisionCredentialMissing { api })?;
        decode(body.expose(), api)
    }
}

fn decode(body: &[u8], api: DecisionApi) -> Result<SecretString> {
    let stored: StoredKey = afd_core::json::object_from_slice(body).map_err(|source| {
        ErrorKind::DecisionCredentialInvalid {
            api,
            source: Unreadable::from(source),
        }
    })?;
    if stored.api_key.is_empty() {
        return Err(ErrorKind::DecisionCredentialMissing { api }.into());
    }
    Ok(stored.api_key)
}
/// Fixed bag names are independent of tenant model names and endpoint text.
const fn bag(api: DecisionApi) -> &'static str {
    match api {
        DecisionApi::Typesafe => "typesafe",
        DecisionApi::Openai => "openai",
        DecisionApi::Perplexity => "perplexity",
        DecisionApi::Cloudflare => "cloudflare",
    }
}
