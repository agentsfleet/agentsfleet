//! SDK signing and I/O reuse the daemon's guarded reqwest client.

use super::R2CheckpointStore;
use crate::{Result, task::TaskRefusal};
use af_crypto::secret::SecretString;
use afd_vault::bag::{Bag, BagFault, secret};
use garde::Validate;
use object_store::{
    ClientOptions, RetryConfig,
    aws::{AmazonS3, AmazonS3Builder, S3ConditionalPut},
    client::{HttpClient, HttpConnector},
};
use serde::Deserialize;

#[derive(Clone, Debug)]
struct Guarded(reqwest::Client);

impl HttpConnector for Guarded {
    fn connect(&self, _options: &ClientOptions) -> object_store::Result<HttpClient> {
        Ok(HttpClient::new(self.0.clone()))
    }
}

/// The R2 access pair as its admin-vault row stores it. Unknown fields are
/// refused so a misspelt key half is not silently dropped.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredR2 {
    access_key_id: Option<SecretString>,
    secret_access_key: Option<SecretString>,
}

/// The longest key half R2 issues, declared once on the type that guards it.
#[derive(Validate)]
struct Within<'a> {
    #[garde(length(bytes, min = 1, max = 256))]
    access_key_id: &'a str,
    #[garde(length(bytes, min = 1, max = 256))]
    secret_access_key: &'a str,
}

/// The scoped R2 access pair checkpoint transfers sign with.
struct R2Keys {
    access_key_id: SecretString,
    secret_access_key: SecretString,
}

impl Bag for R2Keys {
    type Stored = StoredR2;

    fn from_stored(stored: StoredR2) -> Result<Self, BagFault> {
        let access_key_id = secret(stored.access_key_id, "access_key_id")?;
        let secret_access_key = secret(stored.secret_access_key, "secret_access_key")?;
        garde::Unvalidated::new(Within {
            access_key_id: access_key_id.expose(),
            secret_access_key: secret_access_key.expose(),
        })
        .validate()
        .map_err(|report| BagFault::Unparsed(field_at_fault(&report)))?;
        Ok(Self {
            access_key_id,
            secret_access_key,
        })
    }
}

/// The key half a bound report names; the first one when both are over.
fn field_at_fault(report: &garde::Report) -> &'static str {
    let named = report.iter().next().map(|(path, _error)| path.to_string());
    match named.as_deref() {
        Some("secret_access_key") => "secret_access_key",
        _unnamed => "access_key_id",
    }
}

/// A checkpoint bag that exists and cannot be used leaves a line saying why.
const EVENT_BAG_UNUSABLE: &str = "checkpoint_storage_bag_unusable";

impl R2CheckpointStore {
    /// A bag that cannot be used refuses the transfer; an absent one is a
    /// deployment with no storage, and a malformed one says which field.
    fn refuse(&self, fault: BagFault) -> TaskRefusal {
        if fault != BagFault::Absent {
            let error_code = TaskRefusal::StorageUnavailable.code().as_str();
            fault.warn(EVENT_BAG_UNUSABLE, &self.config.credential_name, error_code);
        }
        TaskRefusal::StorageUnavailable
    }

    pub(super) async fn backend(&self) -> Result<AmazonS3> {
        let credentials = self
            .vault
            .bag::<R2Keys>(Some(&self.admin_workspace), &self.config.credential_name)
            .await?
            .map_err(|fault| self.refuse(fault))?;
        Ok(AmazonS3Builder::new()
            .with_endpoint(&self.config.endpoint)
            .with_bucket_name(&self.config.bucket)
            .with_region("auto")
            .with_access_key_id(credentials.access_key_id.expose())
            .with_secret_access_key(credentials.secret_access_key.expose())
            .with_virtual_hosted_style_request(false)
            .with_conditional_put(S3ConditionalPut::ETagMatch)
            .with_http_connector(Guarded(self.client.clone()))
            .with_retry(RetryConfig {
                max_retries: 0,
                retry_timeout: std::time::Duration::from_secs(60),
                ..RetryConfig::default()
            })
            .build()?)
    }
}

#[cfg(test)]
#[path = "backend_tests.rs"]
mod tests;
