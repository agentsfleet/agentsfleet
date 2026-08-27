//! Snapshot and catalogue persistence after pure validation.

use std::sync::Arc;

use bytes::Bytes;
use object_store::path::Path as StorePath;
use object_store::{ObjectStore, ObjectStoreExt as _};

use crate::{Error, ImportBody, PreparedBundle, Result, canonical_snapshot, prepare};

/// Metadata write boundary implemented by the Postgres catalogue repository.
pub trait BundleCatalog: Send + Sync {
    /// Atomically inserts all metadata derived for one validated bundle.
    fn insert(
        &self,
        body: &ImportBody,
        bundle: &PreparedBundle,
    ) -> impl std::future::Future<Output = Result<()>> + Send;
}

/// Coordinates immutable object storage with catalogue persistence.
#[derive(Debug)]
pub struct ImportService<C> {
    store: Option<Arc<dyn ObjectStore>>,
    catalog: C,
}

impl<C> ImportService<C>
where
    C: BundleCatalog,
{
    /// Builds an importer from the crate-native store trait and a catalogue.
    pub fn new(store: Arc<dyn ObjectStore>, catalog: C) -> Self {
        Self {
            store: Some(store),
            catalog,
        }
    }

    /// Builds an importer for skill-only bundles in a deployment without R2.
    pub fn without_store(catalog: C) -> Self {
        Self {
            store: None,
            catalog,
        }
    }

    /// Validates, stores the immutable snapshot, then commits metadata.
    ///
    /// Validation failure performs no I/O. Storage failure cannot leave a
    /// catalogue row. A catalogue failure triggers best-effort removal of its
    /// unreferenced content-addressed object and preserves the catalogue cause.
    ///
    /// # Errors
    /// Returns typed validation, snapshot, storage, or catalogue failures.
    pub async fn import(&self, body: &ImportBody) -> Result<PreparedBundle> {
        tracing::info!(event = "bundle_import_started", source_kind = ?body.source_kind);
        let result = self.import_validated(body).await;
        match &result {
            Ok(bundle) => {
                tracing::info!(event = "bundle_import_succeeded", bundle_name = %bundle.name, content_hash = %bundle.content_hash);
            }
            Err(error) => {
                tracing::warn!(event = "bundle_import_failed", error_code = %error.code(), error = %error);
            }
        }
        result
    }

    async fn import_validated(&self, body: &ImportBody) -> Result<PreparedBundle> {
        let prepared = prepare(body)?;
        let stored = if body.support_files.is_empty() {
            None
        } else {
            let store = self.store.as_ref().ok_or_else(Error::storage_unavailable)?;
            let snapshot = canonical_snapshot(body)?;
            let key = StorePath::from(prepared.snapshot_key.as_str());
            store.put(&key, snapshot.into()).await?;
            Some((store, key))
        };
        if let Err(error) = self.catalog.insert(body, &prepared).await {
            if let Some((store, key)) = stored {
                let _cleanup = store.delete(&key).await;
            }
            return Err(error);
        }
        Ok(prepared)
    }

    /// Fetches snapshot bytes through the same object-store seam used to write.
    ///
    /// # Errors
    /// Preserves the object-store error as its source.
    pub async fn snapshot(&self, key: &str) -> Result<Bytes> {
        self.store
            .as_ref()
            .ok_or_else(Error::storage_unavailable)?
            .get(&StorePath::from(key))
            .await?
            .bytes()
            .await
            .map_err(Error::from)
    }
}

#[cfg(test)]
mod tests;
