//! Private immutable R2 custody; the task lifecycle owns every authorization.

mod backend;
mod bounded;
mod signing;
mod transfer;

pub(in crate::task) use transfer::StorageSession;

use afd_core::id::Uuid7;
use afd_credential::vault::Vault;
use afd_crypto::secret::Kek;
use garde::Validate;
use std::sync::Arc;

/// Platform-owned location and Secret reference, never selected by a fleet.
#[derive(Debug, Clone, Validate, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckpointStorageConfig {
    /// Exact account endpoint; public bucket URLs and arbitrary hosts are refused.
    #[garde(
        length(bytes, max = 160),
        pattern(r"^https://[a-f0-9]{32}(\.(eu|fedramp))?\.r2\.cloudflarestorage\.com/?$")
    )]
    pub endpoint: String,
    /// Deployment's private bucket.
    #[garde(
        length(bytes, min = 3, max = 63),
        pattern(r"^[a-z0-9][a-z0-9.-]+[a-z0-9]$")
    )]
    pub bucket: String,
    /// Existing platform-admin vault row holding scoped R2 access credentials.
    #[garde(length(bytes, min = 1, max = 255), custom(afd_validate::nul_free))]
    pub credential_name: String,
}

/// Existing SDK transport and vault envelope, shared across task operations.
#[derive(Clone)]
pub struct R2CheckpointStore {
    config: garde::Valid<CheckpointStorageConfig>,
    bucket_reference: String,
    admin_workspace: Uuid7,
    vault: Vault,
    pub(super) master: Arc<Kek>,
    client: reqwest::Client,
    verified: Arc<tokio::sync::OnceCell<()>>,
}

impl std::fmt::Debug for R2CheckpointStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("R2CheckpointStore(private)")
    }
}

impl R2CheckpointStore {
    pub(in crate::task) fn bucket_reference(&self) -> &str {
        &self.bucket_reference
    }

    /// One vault read and SDK client per bounded transfer batch.
    pub(in crate::task) async fn session(&self) -> crate::Result<StorageSession<'_>> {
        Ok(StorageSession {
            store: self,
            backend: self.backend().await?,
        })
    }

    /// Uses the existing master key, admin Secret custody and guarded HTTPS client.
    #[must_use]
    pub fn new(
        config: garde::Valid<CheckpointStorageConfig>,
        admin_workspace: Uuid7,
        vault: Vault,
        master: Arc<Kek>,
        client: reqwest::Client,
    ) -> Self {
        let bucket_reference = afd_core::etag::sha256_hex(
            format!(
                "{}/{}",
                config.endpoint.trim_end_matches('/'),
                config.bucket
            )
            .as_bytes(),
        );
        Self {
            config,
            bucket_reference,
            admin_workspace,
            vault,
            master,
            client,
            verified: Arc::new(tokio::sync::OnceCell::new()),
        }
    }
}
