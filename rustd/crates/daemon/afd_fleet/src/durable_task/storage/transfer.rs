//! Exact immutable objects, signed descriptor headers and the bucket's probes.

use super::{R2CheckpointStore, bounded};
use crate::{Result, durable_task::DurableTaskRefusal};
use af_core::{clock, id::Uuid7};
use base64::{Engine as _, prelude::BASE64_STANDARD};
use bytes::Bytes;
use futures_util::StreamExt as _;
use object_store::{
    ObjectStore as _, ObjectStoreExt as _, PutMode, PutOptions,
    aws::AmazonS3,
    path::Path,
    signer::{Method, Signer as _},
};
use serde::Serialize;
use std::{collections::BTreeMap, time::Duration};

/// Transfer material is supervisor-only, never model-visible activity.
#[derive(Serialize)]
pub(crate) struct Authorization {
    pub url: String,
    pub headers: BTreeMap<String, String>,
}

impl std::fmt::Debug for Authorization {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Authorization(redacted)")
    }
}

pub(crate) struct StorageSession<'a> {
    pub(super) store: &'a R2CheckpointStore,
    pub(super) backend: AmazonS3,
}

impl R2CheckpointStore {
    /// Proves, once, before checkpoints activate, that the configured private
    /// bucket refuses a second create of one name and a body unlike the
    /// checksum its link binds. Nothing reads an object back, so those two
    /// answers are what stand behind every accepted upload.
    ///
    /// # Errors
    /// A backend that accepts a replacement or ignores a bound checksum stays
    /// unavailable for checkpoints.
    pub async fn verify_bucket(&self) -> Result<()> {
        self.session().await?.verify_bucket().await
    }
}

impl StorageSession<'_> {
    async fn verify_bucket(&self) -> Result<()> {
        bounded::operation("verify_bucket", async {
            self.store
                .verified
                .get_or_try_init(async || {
                    self.verify_create_only().await?;
                    self.verify_checksum().await
                })
                .await
                .copied()
        })
        .await
    }

    async fn verify_create_only(&self) -> Result<()> {
        let path = probe_path()?;
        let initial = Bytes::from_static(b"create-only-one");
        let create = || PutOptions {
            mode: PutMode::Create,
            ..PutOptions::default()
        };
        let first = self
            .backend
            .put_opts(&path, initial.clone().into(), create())
            .await?;
        let second = self
            .backend
            .put_opts(
                &path,
                Bytes::from_static(b"replacement-two").into(),
                create(),
            )
            .await;
        let rejected = matches!(
            second,
            Err(object_store::Error::AlreadyExists { .. }
                | object_store::Error::Precondition { .. })
        );
        let bytes = read_bounded(
            &self.backend,
            &path,
            u64::try_from(initial.len()).unwrap_or(u64::MAX),
        )
        .await?;
        self.backend.delete(&path).await?;
        if !rejected || bytes.as_deref() != Some(initial.as_ref()) || first.e_tag.is_none() {
            return Err(DurableTaskRefusal::StorageUnavailable.into());
        }
        Ok(())
    }

    /// Sends, through a link signed as every upload's is, a body whose
    /// SHA-256 is not the one the link binds. A bucket that stores it would
    /// store whatever a runner sent, so checkpoints stay unavailable.
    async fn verify_checksum(&self) -> Result<()> {
        let path = probe_path()?;
        let (bound, sent) = (b"checksum-bound-one", b"checksum-other-two");
        let length = u64::try_from(sent.len()).unwrap_or(u64::MAX);
        let headers = upload_headers(&af_core::etag::sha256_hex(bound), length)?;
        let url =
            super::signing::upload(self, &path, &headers, crate::durable_task::checkpoint::LINK)
                .await?;
        let mut put = self.store.client.put(url).body(sent.as_slice());
        for (name, value) in &headers {
            put = put.header(name, value);
        }
        if put.send().await?.status().is_success() {
            self.backend.delete(&path).await?;
            let error_code = af_core::error_code::DURABLE_TASK_STORAGE_UNAVAILABLE;
            tracing::error!(event = "checkpoint_bucket_ignores_checksum", %error_code);
            return Err(DurableTaskRefusal::StorageUnavailable.into());
        }
        Ok(())
    }

    pub(crate) async fn upload(
        &self,
        workspace: &Uuid7,
        object: &Uuid7,
        ciphertext_digest: &str,
        ciphertext_length: u64,
        lifetime: Duration,
    ) -> Result<Authorization> {
        bounded::operation("authorize_upload", async {
            self.verify_bucket().await?;
            let headers = upload_headers(ciphertext_digest, ciphertext_length)?;
            let url =
                super::signing::upload(self, &object_path(workspace, object), &headers, lifetime)
                    .await?;
            Ok(Authorization { url, headers })
        })
        .await
    }

    pub(crate) async fn download(
        &self,
        workspace: &Uuid7,
        object: &Uuid7,
        lifetime: Duration,
    ) -> Result<Authorization> {
        bounded::operation("authorize_download", async {
            let url = self
                .backend
                .signed_url(Method::GET, &object_path(workspace, object), lifetime)
                .await?;
            Ok(Authorization {
                url: url.to_string(),
                headers: BTreeMap::new(),
            })
        })
        .await
    }

    pub(crate) async fn delete(&self, workspace: &Uuid7, object: &Uuid7) -> Result<()> {
        bounded::operation("delete_object", async {
            match self.backend.delete(&object_path(workspace, object)).await {
                Ok(()) | Err(object_store::Error::NotFound { .. }) => Ok(()),
                Err(error) => Err(error.into()),
            }
        })
        .await
    }
}

async fn read_bounded(backend: &AmazonS3, path: &Path, length: u64) -> Result<Option<Vec<u8>>> {
    let tag = u64::try_from(af_crypto::TAG_LEN)
        .map_err(|_overflow| DurableTaskRefusal::CheckpointIncompatible)?;
    if length > af_wire::durable_task::CHECKPOINT_OBJECT_BYTES + tag {
        return Err(DurableTaskRefusal::CheckpointIncompatible.into());
    }
    let response = match backend.get(path).await {
        Ok(response) => response,
        Err(object_store::Error::NotFound { .. }) => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if response.meta.size != length {
        return Err(DurableTaskRefusal::CheckpointIncompatible.into());
    }
    let mut stream = response.into_stream();
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        if u64::try_from(bytes.len())
            .unwrap_or(u64::MAX)
            .saturating_add(u64::try_from(chunk.len()).unwrap_or(u64::MAX))
            > length
        {
            return Err(DurableTaskRefusal::CheckpointIncompatible.into());
        }
        bytes.extend_from_slice(&chunk);
    }
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) != length {
        return Err(DurableTaskRefusal::CheckpointIncompatible.into());
    }
    Ok(Some(bytes))
}

/// What every upload link binds: a create-only write of exactly
/// `length` bytes whose SHA-256 is `digest`, given in hexadecimal.
fn upload_headers(digest: &str, length: u64) -> Result<BTreeMap<String, String>> {
    let digest =
        hex::decode(digest).map_err(|_invalid| DurableTaskRefusal::CheckpointIncompatible)?;
    Ok(BTreeMap::from([
        ("if-none-match".to_owned(), "*".to_owned()),
        ("content-length".to_owned(), length.to_string()),
        (
            "x-amz-checksum-sha256".to_owned(),
            BASE64_STANDARD.encode(digest),
        ),
    ]))
}

/// A fresh name under which a probe writes, and then removes, its bytes.
fn probe_path() -> Result<Path> {
    let id = af_crypto::entropy::Entropy::new().uuid7(clock::now())?;
    Ok(Path::from(format!("capability-probes/{}", id.as_str())))
}

fn object_path(workspace: &Uuid7, object: &Uuid7) -> Path {
    Path::from(format!(
        "workspaces/{}/objects/{}",
        workspace.as_str(),
        object.as_str()
    ))
}

#[cfg(test)]
#[path = "transfer_tests.rs"]
mod tests;
