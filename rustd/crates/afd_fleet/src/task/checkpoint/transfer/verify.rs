//! Remote bytes become verified only after ciphertext and plaintext authenticate.

use super::super::row::Plan;
use crate::{
    Result,
    error::query,
    task::{TaskRefusal, Tasks, storage::StorageSession, transaction},
};
use afd_core::id::Uuid7;
use afd_crypto::{
    checkpoint::{CheckpointScope, EncryptedCheckpointObject},
    secret::SecretBytes,
};
use afd_wire::task::CheckpointObject;
use serde::Serialize;
use sqlx::types::Json;

#[derive(Serialize)]
pub(super) struct Receipt {
    id: String,
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
    let digest = object
        .ciphertext_sha256
        .as_deref()
        .ok_or(TaskRefusal::CheckpointIncompatible)?;
    let length = object
        .ciphertext_length
        .ok_or(TaskRefusal::CheckpointIncompatible)?;
    let Some(mut bytes) = session
        .read_object(workspace, identity, id, digest, length)
        .await?
    else {
        return Ok(None);
    };
    let split = bytes
        .len()
        .checked_sub(afd_crypto::TAG_LEN)
        .ok_or(TaskRefusal::CheckpointIncompatible)?;
    let tag: [u8; afd_crypto::TAG_LEN] = bytes
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
    let nonce: [u8; afd_crypto::NONCE_LEN] = nonce
        .try_into()
        .map_err(|_invalid| TaskRefusal::CheckpointIncompatible)?;
    let plaintext = EncryptedCheckpointObject::from_parts(bytes, tag).open(
        SecretBytes::new(key.expose().to_vec()),
        &nonce,
        CheckpointScope {
            workspace_id: workspace,
            repository: &plan.repository,
            object_id: id,
            format_version: u32::try_from(plan.format_version)
                .map_err(|_overflow| TaskRefusal::CheckpointIncompatible)?,
        },
    )?;
    if plaintext.expose().len() as u64 != object.plaintext.size
        || afd_core::etag::sha256_hex(plaintext.expose()) != object.plaintext.sha256
    {
        return Err(TaskRefusal::CheckpointIncompatible.into());
    }
    Ok(Some(Receipt {
        id: id.as_str().to_owned(),
        digest: digest.to_owned(),
        length: i64::try_from(length).map_err(|_overflow| TaskRefusal::CheckpointIncompatible)?,
    }))
}

pub(super) async fn record(tasks: &Tasks, workspace: &Uuid7, receipts: &[Receipt]) -> Result<()> {
    if receipts.is_empty() {
        return Ok(());
    }
    let mut connection = transaction::acquire(tasks.leases.pool()).await?;
    let mut tx = transaction::begin(&mut connection).await?;
    let parent: Option<String> = sqlx::query_scalar(include_str!("../sql/lock_workspace.sql"))
        .bind(workspace.as_str())
        .fetch_optional(&mut *tx)
        .await
        .map_err(query("hold workspace while accounting verified storage"))?;
    if parent.is_some() {
        super::super::identity::quota(&mut tx, workspace.as_str()).await?;
        sqlx::query(include_str!("../sql/verify.sql"))
            .bind(workspace.as_str())
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
