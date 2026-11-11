//! Decision calls spend the deployment's own key, read again for each dispatch.

use af_core::error_code;
use af_core::id::Uuid7;
use af_crypto::secret::SecretString;
use af_wire::decision::DecisionApi;
use afd_vault::bag::{Bag, BagFault, secret};
use serde::Deserialize;

use super::Providers;
use crate::{Result, error::ErrorKind};

/// The one field a decision call spends; unrelated fields stay in the vault.
/// A typed secret guards the parsed allocation without building a JSON tree.
///
/// Required rather than optional: a body with no `api_key` was configured
/// wrongly, where an empty one was never configured, and the two are reported
/// apart.
#[derive(Deserialize)]
struct StoredKey {
    api_key: SecretString,
}

/// A decision provider's key, as the `afd_vault::bag` standard reads it.
struct DecisionKey(SecretString);

impl Bag for DecisionKey {
    type Stored = StoredKey;

    fn from_stored(stored: StoredKey) -> core::result::Result<Self, BagFault> {
        secret(Some(stored.api_key), "api_key").map(Self)
    }
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
        let read = self
            .vault()
            .bag::<DecisionKey>(Some(admin), bag(api))
            .await?;
        read.map(|key| key.0).map_err(|fault| refused(api, fault))
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
        let read = self
            .vault()
            .bag_on::<DecisionKey>(connection, admin, bag(api))
            .await?;
        read.map(|key| key.0).map_err(|fault| refused(api, fault))
    }
}

/// An absent or empty key is a key nobody configured; a body that is not the
/// declared shape is a key someone configured wrongly, and says so in the log.
fn refused(api: DecisionApi, fault: BagFault) -> crate::Error {
    match fault {
        BagFault::Absent => ErrorKind::DecisionCredentialMissing { api }.into(),
        BagFault::Missing(_) => {
            warn(api, fault);
            ErrorKind::DecisionCredentialMissing { api }.into()
        }
        BagFault::Malformed | BagFault::Unparsed(_) => {
            warn(api, fault);
            ErrorKind::DecisionCredentialInvalid { api, fault }.into()
        }
    }
}

/// Names the bag and the fault, never a value.
fn warn(api: DecisionApi, fault: BagFault) {
    let error_code = error_code::INTERNAL_OPERATION_FAILED.as_str();
    fault.warn("decision_credential_unusable", bag(api), error_code);
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
