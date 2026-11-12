//! Open only the credential whose current metadata still equals the admitted snapshot.

use af_core::id::Uuid7;
use af_crypto::secret::SecretString;
use serde::Deserialize;
use sqlx::PgConnection;

use super::{RegisteredModel, read};
use crate::{
    error::{Result, provider_malformed, provider_secret_missing},
    provider::{Providers, credential},
};

/// The field a registered model credential carries its key under.
const FIELD_API_KEY: &str = "api_key";

#[derive(Deserialize)]
struct Key {
    api_key: SecretString,
}

impl Providers {
    /// Opens an explicitly selected, still-current owner credential in the caller's transaction.
    ///
    /// No pool acquisition, runtime-default fallback or destination is accepted here.
    /// The caller must commit dispatch intent before using the returned key.
    ///
    /// # Errors
    /// Preserves store/decryption failures; stale metadata answers `None`. A
    /// locked row that has vanished breaks the lock's guarantee and is an error.
    pub async fn registered_key(
        &self,
        connection: &mut PgConnection,
        owner: &Uuid7,
        pinned: &RegisteredModel,
    ) -> Result<Option<SecretString>> {
        let Some(registration) = read(connection, owner, &pinned.reference).await? else {
            return Ok(None);
        };
        if registration.model != *pinned {
            return Ok(None);
        }
        let (workspace, name) = (registration.workspace_id, registration.key_name);
        // `read` took this secret `FOR SHARE` in the caller's transaction, so
        // the row it named is still there to open.
        let body = self
            .vault()
            .find_on(connection, &workspace, &name)
            .await?
            .ok_or_else(provider_secret_missing)?;
        let key: Key = credential(body.expose(), FIELD_API_KEY)?;
        if key.api_key.is_empty() {
            return Err(provider_malformed(FIELD_API_KEY));
        }
        Ok(Some(key.api_key))
    }
}
