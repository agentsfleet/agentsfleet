//! Runtime opens the exact owner reference selected by the saved setup.

use af_core::id::Uuid7;
use af_crypto::aad::Aad;
use afd_billing::Posture;
use afd_vault::StoredEnvelope;
use sqlx::Connection as _;

use super::super::registered::{CONTEXT, read};
use super::project;
use crate::{
    error::{Result, provider_secret_missing, query, vault_open},
    provider::cache::PlatformKeyKey,
    provider::{PLATFORM_MODEL_REF, Providers, Resolved, managed, vetted},
    vault::sql,
};

impl Providers {
    /// Resolves a saved reference without consulting the caller's account default.
    ///
    /// # Errors
    /// Missing references, invalid stored credentials and datastore failures are preserved.
    pub async fn resolve_reference(&self, owner: &Uuid7, reference: &str) -> Result<Resolved> {
        self.resolve_read(owner, reference)
            .await
            .map(|(resolved, _observed)| resolved)
    }

    /// [`Self::resolve_reference`], answered from the platform key cache when
    /// `observed` — the versions the caller's claim statement read — names a
    /// held entry.
    ///
    /// Only the platform reference is cached; a tenant entry is read every
    /// time (blocker 2). A miss resolves and holds the result under the
    /// versions its own read returned, so a change between the claim and this
    /// read is cached as the newer resolution, never the older.
    ///
    /// # Errors
    /// As [`Self::resolve_reference`].
    pub async fn resolve_observed(
        &self,
        owner: &Uuid7,
        reference: &str,
        observed: Option<&PlatformKeyKey>,
    ) -> Result<Resolved> {
        let cached = reference == PLATFORM_MODEL_REF && observed.is_some();
        if let Some(key) = observed.filter(|_| cached)
            && let Some(held) = self.platform_keys().get(key).await
        {
            return Ok(held);
        }
        let (resolved, read) = self.resolve_read(owner, reference).await?;
        if cached && let Some(read) = read {
            self.platform_keys().insert(read, resolved.clone()).await;
        }
        Ok(resolved)
    }

    /// The resolution, and the versions its platform read returned.
    async fn resolve_read(
        &self,
        owner: &Uuid7,
        reference: &str,
    ) -> Result<(Resolved, Option<PlatformKeyKey>)> {
        let mut connection = self.pool().acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        let registration = read(&mut tx, owner, reference)
            .await?
            .ok_or_else(provider_secret_missing)?;
        let observed = registration.observed.clone();
        let (workspace, name) = (registration.workspace_id, registration.key_name);
        let model = project(registration.model).ok_or_else(provider_secret_missing)?;
        let body = sqlx::query_as::<_, StoredEnvelope>(sql::SELECT_SECRET)
            .bind(&workspace)
            .bind(&name)
            .fetch_optional(&mut *tx)
            .await
            .map_err(query(CONTEXT))?
            .ok_or_else(provider_secret_missing)?
            .into_envelope()
            .map_err(vault_open)?
            .open(self.vault_key(), &Aad::new(workspace.as_str(), &name))
            .map_err(vault_open)?;
        let credential = vetted::vet(body.expose())?;
        let api_key = managed::bearer(credential.api_key, credential.dialled.is_some())?;
        tx.commit().await.map_err(query(CONTEXT))?;
        let endpoint = Some(super::super::Dialled {
            base_url: model.inference.base_url.as_str().into(),
            inference_host: registration.inference_host,
        });
        let resolved = Resolved::new(
            if reference == PLATFORM_MODEL_REF {
                Posture::Platform
            } else {
                Posture::SelfManaged
            },
            model.provider.into(),
            model.model.into(),
            model.context_cap_tokens,
            endpoint,
            api_key,
        )
        .with_inference(model.inference);
        Ok((resolved, observed))
    }
}
