//! Runtime opens the exact owner reference selected by the saved setup.

use af_core::id::Uuid7;
use afd_billing::Posture;
use sqlx::Connection as _;

use super::super::registered::{CONTEXT, read};
use super::project;
use crate::{
    error::{Result, provider_secret_missing, query},
    provider::{PLATFORM_MODEL_REF, Providers, Resolved, managed, vetted},
};

impl Providers {
    /// Resolves a saved reference without consulting the caller's account default.
    ///
    /// # Errors
    /// Missing references, invalid stored credentials and datastore failures are preserved.
    pub async fn resolve_reference(&self, owner: &Uuid7, reference: &str) -> Result<Resolved> {
        let mut connection = self.pool().acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        let registration = read(&mut tx, owner, reference)
            .await?
            .ok_or_else(provider_secret_missing)?;
        let (workspace, name) = (registration.workspace_id, registration.key_name);
        let model = project(registration.model).ok_or_else(provider_secret_missing)?;
        let body = self
            .vault()
            .find_on(&mut tx, &workspace, &name)
            .await?
            .ok_or_else(provider_secret_missing)?;
        let credential = vetted::vet(body.expose())?;
        let api_key = managed::bearer(credential.api_key, credential.dialled.is_some())?;
        tx.commit().await.map_err(query(CONTEXT))?;
        let endpoint = Some(super::super::Dialled {
            base_url: model.inference.base_url.as_str().into(),
            inference_host: registration.inference_host,
        });
        Ok(Resolved::new(
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
        .with_inference(model.inference))
    }
}
