//! Open only the credential whose current metadata still equals the admitted snapshot.

use afd_core::id::Uuid7;
use afd_crypto::{aad::Aad, secret::SecretString};
use afd_vault::StoredEnvelope;
use serde::Deserialize;
use sqlx::{FromRow as _, Postgres, Row as _, Transaction};

use super::{CONTEXT, RegisteredModel, read};
use crate::{
    error::{Result, provider_malformed, query, vault_open},
    provider::{Providers, credential},
    vault::sql,
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
    /// Preserves store/decryption failures; stale metadata answers `None`.
    pub async fn registered_key(
        &self,
        connection: &mut Transaction<'_, Postgres>,
        owner: &Uuid7,
        pinned: &RegisteredModel,
    ) -> Result<Option<SecretString>> {
        let Some(registration) = read(connection, owner, &pinned.reference).await? else {
            return Ok(None);
        };
        if registration.model != *pinned {
            return Ok(None);
        }
        let row = registration.row;
        let workspace: String = row.try_get(6).map_err(query(CONTEXT))?;
        let name: String = row.try_get(7).map_err(query(CONTEXT))?;
        let row = sqlx::query(sql::SELECT_SECRET)
            .bind(&workspace)
            .bind(&name)
            // The metadata read holds this credential FOR SHARE until dispatch commits.
            .fetch_one(&mut **connection)
            .await
            .map_err(query(CONTEXT))?;
        let body = StoredEnvelope::from_row(&row)
            .map_err(query(CONTEXT))?
            .into_envelope()
            .map_err(vault_open)?
            .open(self.vault_key(), &Aad::new(&workspace, &name))
            .map_err(vault_open)?;
        let key: Key = credential(body.expose(), FIELD_API_KEY)?;
        if key.api_key.is_empty() {
            return Err(provider_malformed(FIELD_API_KEY));
        }
        Ok(Some(key.api_key))
    }
}
