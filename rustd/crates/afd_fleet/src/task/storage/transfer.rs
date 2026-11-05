//! Exact immutable objects, signed descriptor headers and bounded native readback.

use super::{R2CheckpointStore, bounded};
use crate::{Result, task::TaskRefusal};
use afd_core::{clock, id::Uuid7};
use base64::{Engine as _, prelude::BASE64_STANDARD};
use bytes::Bytes;
use futures_util::StreamExt as _;
use object_store::{
    ObjectStore as _, ObjectStoreExt as _, PutMode, PutOptions,
    aws::AmazonS3,
    path::Path,
    signer::{HeaderName, HeaderValue, Method, SignedUrlOptions, Signer as _},
};
use serde::Serialize;
use std::{collections::BTreeMap, time::Duration};

/// Transfer material is supervisor-only, never model-visible activity.
#[derive(Serialize)]
pub(in crate::task) struct Authorization {
    pub url: String,
    pub headers: BTreeMap<String, String>,
}

impl std::fmt::Debug for Authorization {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Authorization(redacted)")
    }
}

pub(in crate::task) struct StorageSession<'a> {
    pub(super) store: &'a R2CheckpointStore,
    pub(super) backend: AmazonS3,
}

impl R2CheckpointStore {
    /// Verifies conditional creation in the configured private bucket before activation.
    ///
    /// # Errors
    /// A backend that accepts replacement stays unavailable for checkpoints.
    pub async fn verify_create_only(&self) -> Result<()> {
        self.session().await?.verify_create_only().await
    }
}

impl StorageSession<'_> {
    async fn verify_create_only(&self) -> Result<()> {
        bounded::operation("verify_create_only", async {
            self.store
                .verified
                .get_or_try_init(|| async {
                    let id = afd_crypto::entropy::Entropy::new().uuid7(clock::now())?;
                    let path = Path::from(format!("capability-probes/{}", id.as_str()));
                    let initial = Bytes::from_static(b"create-only-one");
                    let first = self
                        .backend
                        .put_opts(
                            &path,
                            initial.clone().into(),
                            PutOptions {
                                mode: PutMode::Create,
                                ..PutOptions::default()
                            },
                        )
                        .await?;
                    let second = self
                        .backend
                        .put_opts(
                            &path,
                            Bytes::from_static(b"replacement-two").into(),
                            PutOptions {
                                mode: PutMode::Create,
                                ..PutOptions::default()
                            },
                        )
                        .await;
                    let rejected = matches!(
                        second,
                        Err(object_store::Error::AlreadyExists { .. }
                            | object_store::Error::Precondition { .. })
                    );
                    let bytes = read_bounded(&self.backend, &path, initial.len() as u64).await?;
                    self.backend.delete(&path).await?;
                    if !rejected
                        || bytes.as_deref() != Some(initial.as_ref())
                        || first.e_tag.is_none()
                    {
                        return Err(TaskRefusal::StorageUnavailable.into());
                    }
                    Ok(())
                })
                .await
                .copied()
        })
        .await
    }

    pub(in crate::task) async fn upload(
        &self,
        workspace: &Uuid7,
        repository: &Uuid7,
        object: &Uuid7,
        ciphertext_digest: &str,
        ciphertext_length: u64,
        lifetime: Duration,
    ) -> Result<Authorization> {
        bounded::operation("authorize_upload", async {
            self.verify_create_only().await?;
            let digest = hex::decode(ciphertext_digest)
                .map_err(|_invalid| TaskRefusal::CheckpointIncompatible)?;
            let headers = BTreeMap::from([
                ("if-none-match".to_owned(), "*".to_owned()),
                ("content-length".to_owned(), ciphertext_length.to_string()),
                (
                    "x-amz-checksum-sha256".to_owned(),
                    BASE64_STANDARD.encode(digest),
                ),
            ]);
            let mut options = SignedUrlOptions::new();
            for (name, value) in &headers {
                options = options.with_signed_header(
                    HeaderName::from_bytes(name.as_bytes())
                        .map_err(|_invalid| TaskRefusal::StorageUnavailable)?,
                    HeaderValue::from_str(value)
                        .map_err(|_invalid| TaskRefusal::StorageUnavailable)?,
                );
            }
            let url = self
                .backend
                .signed_url_opts(
                    Method::PUT,
                    &object_path(workspace, repository, object),
                    lifetime,
                    &options,
                )
                .await?;
            Ok(Authorization {
                url: url.to_string(),
                headers,
            })
        })
        .await
    }

    pub(in crate::task) async fn download(
        &self,
        workspace: &Uuid7,
        repository: &Uuid7,
        object: &Uuid7,
        lifetime: Duration,
    ) -> Result<Authorization> {
        bounded::operation("authorize_download", async {
            let url = self
                .backend
                .signed_url(
                    Method::GET,
                    &object_path(workspace, repository, object),
                    lifetime,
                )
                .await?;
            Ok(Authorization {
                url: url.to_string(),
                headers: BTreeMap::new(),
            })
        })
        .await
    }

    pub(in crate::task) async fn read_object(
        &self,
        workspace: &Uuid7,
        repository: &Uuid7,
        object: &Uuid7,
        digest: &str,
        length: u64,
    ) -> Result<Option<Vec<u8>>> {
        bounded::operation("verify_object", async {
            let bytes = read_bounded(
                &self.backend,
                &object_path(workspace, repository, object),
                length,
            )
            .await?;
            if bytes
                .as_ref()
                .is_some_and(|bytes| afd_core::etag::sha256_hex(bytes) != digest)
            {
                return Err(TaskRefusal::CheckpointIncompatible.into());
            }
            Ok(bytes)
        })
        .await
    }

    pub(in crate::task) async fn delete(
        &self,
        workspace: &Uuid7,
        repository: &Uuid7,
        object: &Uuid7,
    ) -> Result<()> {
        bounded::operation("delete_object", async {
            match self
                .backend
                .delete(&object_path(workspace, repository, object))
                .await
            {
                Ok(()) | Err(object_store::Error::NotFound { .. }) => Ok(()),
                Err(error) => Err(error.into()),
            }
        })
        .await
    }
}

async fn read_bounded(backend: &AmazonS3, path: &Path, length: u64) -> Result<Option<Vec<u8>>> {
    if length > afd_wire::task::CHECKPOINT_OBJECT_BYTES + afd_crypto::TAG_LEN as u64 {
        return Err(TaskRefusal::CheckpointIncompatible.into());
    }
    let response = match backend.get(path).await {
        Ok(response) => response,
        Err(object_store::Error::NotFound { .. }) => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if response.meta.size != length {
        return Err(TaskRefusal::CheckpointIncompatible.into());
    }
    let mut stream = response.into_stream();
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        if (bytes.len() as u64).saturating_add(chunk.len() as u64) > length {
            return Err(TaskRefusal::CheckpointIncompatible.into());
        }
        bytes.extend_from_slice(&chunk);
    }
    if bytes.len() as u64 != length {
        return Err(TaskRefusal::CheckpointIncompatible.into());
    }
    Ok(Some(bytes))
}

fn object_path(workspace: &Uuid7, repository: &Uuid7, object: &Uuid7) -> Path {
    Path::from(format!(
        "workspaces/{}/repositories/{}/objects/{}",
        workspace.as_str(),
        repository.as_str(),
        object.as_str()
    ))
}
