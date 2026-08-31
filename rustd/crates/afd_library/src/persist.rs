//! Snapshot and catalogue persistence after pure validation.

use std::sync::Arc;

use bytes::Bytes;
use object_store::path::Path as StorePath;
use object_store::{ObjectStore, ObjectStoreExt as _};

use crate::{Error, ImportBody, PreparedBundle, Result, canonical_snapshot, prepare};

/// One bundle as it now stands in a catalogue.
///
/// Carries the row's IDENTIFIER beside the validated bundle, because the two
/// tiers do not agree on what that is: the platform catalogue is keyed by the
/// bundle's own name, and a workspace's library mints a UUID. A caller that
/// derived the id from the bundle would be right on one tier and wrong on the
/// other, which is exactly the kind of thing a return value should settle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Onboarded {
    /// The row's identifier, as the catalogue that wrote it knows it.
    pub id: String,
    /// The validated bundle behind it.
    pub bundle: PreparedBundle,
}

/// Metadata write boundary implemented by the Postgres catalogue repository.
pub trait BundleCatalog: Send + Sync {
    /// Atomically inserts all metadata derived for one validated bundle.
    ///
    /// Answers the row's identifier — the one that now stands, which on a
    /// re-onboard is the identifier the FIRST one minted rather than a fresh
    /// one this call would have used.
    fn insert(
        &self,
        body: &ImportBody,
        bundle: &PreparedBundle,
    ) -> impl std::future::Future<Output = Result<String>> + Send;
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
    /// catalogue row. A catalogue failure preserves the immutable snapshot:
    /// another concurrent import may already reference the same content hash.
    ///
    /// # Errors
    /// Returns typed validation, snapshot, storage, or catalogue failures.
    pub async fn import(&self, body: &ImportBody) -> Result<Onboarded> {
        tracing::info!(event = "bundle_import_started", source_kind = ?body.source_kind);
        let result = self.import_validated(body).await;
        match &result {
            Ok(onboarded) => {
                let bundle = &onboarded.bundle;
                tracing::info!(event = "bundle_import_completed", bundle_name = %bundle.name, content_hash = %bundle.content_hash);
            }
            Err(error) => {
                tracing::warn!(event = "bundle_import_failed", error_code = %error.code(), error = %error);
            }
        }
        result
    }

    async fn import_validated(&self, body: &ImportBody) -> Result<Onboarded> {
        let prepared = prepare(body)?;
        if !body.support_files.is_empty() {
            let store = self.store.as_ref().ok_or_else(Error::storage_unavailable)?;
            let snapshot = canonical_snapshot(body)?;
            let key = StorePath::from(prepared.snapshot_key.as_str());
            store.put(&key, snapshot.into()).await?;
        }
        let id = self.catalog.insert(body, &prepared).await?;
        Ok(Onboarded {
            id,
            bundle: prepared,
        })
    }

    /// Fetches snapshot bytes through the same object-store seam used to write.
    ///
    /// # Errors
    /// Preserves the object-store error as its source.
    pub async fn snapshot(&self, key: &str) -> Result<Bytes> {
        Ok(self
            .store
            .as_ref()
            .ok_or_else(Error::storage_unavailable)?
            .get(&StorePath::from(key))
            .await?
            .bytes()
            .await?)
    }
}

#[cfg(test)]
mod tests;
