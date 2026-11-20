//! Remote bytes become verified only after ciphertext and plaintext authenticate.

use super::super::row::Plan;
use crate::{
    Result,
    error::query,
    task::{TaskRefusal, Tasks, storage::StorageSession, transaction},
};
use af_core::id::Uuid7;
use af_crypto::{
    checkpoint::{CheckpointScope, EncryptedCheckpointObject},
    secret::SecretBytes,
};
use af_wire::task::CheckpointObject;
use serde::Serialize;
use sqlx::types::Json;

#[derive(Serialize)]
pub(crate) struct Receipt {
    id: Uuid7,
    digest: String,
    length: i64,
}

pub(super) async fn remote(
    session: &StorageSession<'_>,
    workspace: &Uuid7,
    identity: &Uuid7,
    id: &Uuid7,
    plan: &Plan,
    object: &CheckpointObject,
    key: &SecretBytes,
) -> Result<Option<Receipt>> {
    remote_scope(
        session,
        identity,
        CheckpointScope {
            workspace_id: workspace,
            repository: &plan.repository,
            object_id: id,
            format_version: u32::try_from(plan.format_version)
                .map_err(|_overflow| TaskRefusal::CheckpointIncompatible)?,
        },
        object,
        key,
    )
    .await
}

pub(crate) async fn remote_scope(
    session: &StorageSession<'_>,
    identity: &Uuid7,
    scope: CheckpointScope<'_>,
    object: &CheckpointObject,
    key: &SecretBytes,
) -> Result<Option<Receipt>> {
    let workspace = scope.workspace_id;
    let id = scope.object_id;
    let digest = object
        .ciphertext_sha256
        .as_deref()
        .ok_or(TaskRefusal::CheckpointIncompatible)?;
    let length = object
        .ciphertext_length
        .ok_or(TaskRefusal::CheckpointIncompatible)?;
    let Some(bytes) = session
        .read_object(workspace, identity, id, digest, length)
        .await?
    else {
        return Ok(None);
    };
    let plaintext = open(scope, object, key, bytes)?;
    if u64::try_from(plaintext.expose().len()).unwrap_or(u64::MAX) != object.plaintext.size
        || af_core::etag::sha256_hex(plaintext.expose()) != object.plaintext.sha256
    {
        return Err(TaskRefusal::CheckpointIncompatible.into());
    }
    Ok(Some(Receipt {
        id: id.clone(),
        digest: digest.to_owned(),
        length: i64::try_from(length).map_err(|_overflow| TaskRefusal::CheckpointIncompatible)?,
    }))
}

fn open(
    scope: CheckpointScope<'_>,
    object: &CheckpointObject,
    key: &SecretBytes,
    mut bytes: Vec<u8>,
) -> Result<SecretBytes> {
    let split = bytes
        .len()
        .checked_sub(af_crypto::TAG_LEN)
        .ok_or(TaskRefusal::CheckpointIncompatible)?;
    let tag: [u8; af_crypto::TAG_LEN] = bytes
        .split_off(split)
        .try_into()
        .map_err(|_invalid| TaskRefusal::CheckpointIncompatible)?;
    let nonce = base64::Engine::decode(
        &base64::prelude::BASE64_STANDARD,
        object
            .nonce
            .as_deref()
            .ok_or(TaskRefusal::CheckpointIncompatible)?,
    )
    .map_err(|_invalid| TaskRefusal::CheckpointIncompatible)?;
    let nonce: [u8; af_crypto::NONCE_LEN] = nonce
        .try_into()
        .map_err(|_invalid| TaskRefusal::CheckpointIncompatible)?;
    Ok(EncryptedCheckpointObject::from_parts(bytes, tag).open(
        SecretBytes::new(key.expose().to_vec()),
        &nonce,
        scope,
    )?)
}

pub(super) async fn record(tasks: &Tasks, workspace: &Uuid7, receipts: &[Receipt]) -> Result<()> {
    if receipts.is_empty() {
        return Ok(());
    }
    let mut connection = transaction::acquire(tasks.runs.pool()).await?;
    let mut tx = transaction::begin(&mut connection).await?;
    let parent: Option<Uuid7> = sqlx::query_scalar(crate::task::sql::LOCK_WORKSPACE)
        .bind(workspace)
        .fetch_optional(&mut *tx)
        .await
        .map_err(query("hold workspace while accounting verified storage"))?;
    if parent.is_some() {
        super::super::identity::quota(&mut tx, workspace).await?;
        sqlx::query(super::super::VERIFY)
            .bind(workspace)
            .bind(Json(receipts))
            .execute(&mut *tx)
            .await
            .map_err(query("account verified unique objects once"))?;
    }
    tx.commit()
        .await
        .map_err(query("commit independently confirmed storage custody"))?;
    transaction::recycle(connection);
    Ok(())
}
