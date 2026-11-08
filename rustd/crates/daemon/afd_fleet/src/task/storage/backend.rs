//! SDK signing and I/O reuse the daemon's guarded reqwest client.

use super::R2CheckpointStore;
use crate::{Result, task::TaskRefusal};
use afd_credential::vault::KeyRef;
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

#[derive(Deserialize, Validate)]
#[serde(deny_unknown_fields)]
struct Credentials<'a> {
    #[garde(length(bytes, min = 1, max = 256))]
    access_key_id: &'a str,
    #[garde(length(bytes, min = 1, max = 256))]
    secret_access_key: &'a str,
}

impl R2CheckpointStore {
    pub(super) async fn backend(&self) -> Result<AmazonS3> {
        let secret = self
            .vault
            .open(KeyRef {
                workspace_id: &self.admin_workspace,
                name: &self.config.credential_name,
            })
            .await?
            .ok_or(TaskRefusal::StorageUnavailable)?;
        let credentials: Credentials<'_> =
            serde_json::from_slice(secret.expose()).map_err(af_core::json::Unreadable::from)?;
        let credentials = garde::Unvalidated::new(credentials)
            .validate()
            .map_err(|_invalid| TaskRefusal::StorageUnavailable)?;
        Ok(AmazonS3Builder::new()
            .with_endpoint(&self.config.endpoint)
            .with_bucket_name(&self.config.bucket)
            .with_region("auto")
            .with_access_key_id(credentials.access_key_id)
            .with_secret_access_key(credentials.secret_access_key)
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
