//! Runtime opens the exact owner reference selected by the saved setup.

use afd_billing::Posture;
use afd_core::id::Uuid7;
use afd_crypto::aad::Aad;
use afd_vault::StoredEnvelope;
use sqlx::Connection as _;

use super::{CONTEXT, read};
use crate::{
    error::{Result, provider_secret_missing, query, vault_open},
    provider::{PLATFORM_MODEL_REF, Providers, Resolved, managed, vetted},
    vault::sql,
};

impl Providers {
    /// Resolves a saved reference without consulting the caller's account default.
    ///
    /// # Errors
    /// Missing references, invalid stored credentials and datastore failures are preserved.
    pub async fn resolve_reference(&self, owner: &Uuid7, reference: &str) -> Result<Resolved> {
        let mut connection = self.pool().acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        let row = read(&mut tx, owner, reference)
            .await?
            .ok_or_else(provider_secret_missing)?;
        let model = row.metadata(reference);
        let (workspace, name) = (row.workspace_id, row.key_name);
        let body = sqlx::query_as::<_, StoredEnvelope>(sql::SELECT_SECRET)
            .bind(&workspace)
            .bind(&name)
            .fetch_optional(&mut *tx)
            .await
            .map_err(query(CONTEXT))?
            .ok_or_else(provider_secret_missing)?
            .into_envelope()
            .map_err(vault_open)?
            .open(self.vault_key(), &Aad::new(&workspace, &name))
            .map_err(vault_open)?;
        let credential = vetted::vet(body.expose())?;
        let api_key = managed::bearer(credential.api_key, credential.dialled.is_some())?;
        tx.commit().await.map_err(query(CONTEXT))?;
        Ok(Resolved::new(
            if reference == PLATFORM_MODEL_REF {
                Posture::Platform
            } else {
                Posture::SelfManaged
            },
            model.provider.into(),
            model.model.into(),
            model.context_cap_tokens,
            credential.dialled,
            api_key,
        ))
    }
}
