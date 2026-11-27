//! Remote bytes become verified only after ciphertext and plaintext authenticate.

use super::super::row::Plan;
use crate::{
    Result,
    durable_task::{
        DurableTaskRefusal, DurableTasks,
        row::{small_counter, sql_counter},
        storage::{StorageSession, WorkspaceKey, chunk_cipher},
        transaction,
    },
    error::query,
};
use af_core::id::Uuid7;
use af_crypto::{checkpoint::CheckpointScope, secret::SecretBytes};
use af_wire::durable_task::CheckpointObject;
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
    id: &Uuid7,
    plan: &Plan,
    object: &CheckpointObject,
    key: &WorkspaceKey,
) -> Result<Option<Receipt>> {
    remote_scope(
        session,
        CheckpointScope {
            workspace_id: &plan.workspace_id,
            repository: &plan.repository,
            object_id: id,
            format_version: small_counter(plan.format_version, "checkpoint format_version")?,
        },
        object,
        key,
    )
    .await
}

pub(crate) async fn remote_scope(
    session: &StorageSession<'_>,
    scope: CheckpointScope<'_>,
    object: &CheckpointObject,
    key: &WorkspaceKey,
) -> Result<Option<Receipt>> {
    let workspace = scope.workspace_id;
    let id = scope.object_id;
    let digest = object
        .ciphertext_sha256
        .as_deref()
        .ok_or(DurableTaskRefusal::CheckpointIncompatible)?;
    let length = object
        .ciphertext_length
        .ok_or(DurableTaskRefusal::CheckpointIncompatible)?;
    let Some(bytes) = session.read_object(workspace, id, digest, length).await? else {
        return Ok(None);
    };
    let plaintext = open(scope, object, key, bytes)?;
    if u64::try_from(plaintext.expose().len()).unwrap_or(u64::MAX) != object.plaintext.size
        || af_core::etag::sha256_hex(plaintext.expose()) != object.plaintext.sha256
    {
        return Err(DurableTaskRefusal::CheckpointIncompatible.into());
    }
    Ok(Some(Receipt {
        id: id.clone(),
        digest: digest.to_owned(),
        length: sql_counter(length, "ciphertext length")?,
    }))
}

/// Opens stored bytes under the cipher the object was registered with.
fn open(
    scope: CheckpointScope<'_>,
    object: &CheckpointObject,
    key: &WorkspaceKey,
    bytes: Vec<u8>,
) -> Result<SecretBytes> {
    let nonce = base64::Engine::decode(
        &base64::prelude::BASE64_STANDARD,
        object
            .nonce
            .as_deref()
            .ok_or(DurableTaskRefusal::CheckpointIncompatible)?,
    )
    .map_err(|_invalid| DurableTaskRefusal::CheckpointIncompatible)?;
    let nonce: [u8; af_crypto::NONCE_LEN] = nonce
        .try_into()
        .map_err(|_invalid| DurableTaskRefusal::CheckpointIncompatible)?;
    Ok(chunk_cipher(object.cipher).open(key.bytes(), &nonce, &scope.binding(), bytes)?)
}

pub(super) async fn record(
    tasks: &DurableTasks,
    workspace: &Uuid7,
    receipts: &[Receipt],
) -> Result<()> {
    if receipts.is_empty() {
        return Ok(());
    }
    let mut connection = transaction::acquire(tasks.runs.pool()).await?;
    let mut tx = transaction::begin(&mut connection).await?;
    let parent: Option<Uuid7> = sqlx::query_scalar(crate::durable_task::sql::LOCK_WORKSPACE)
        .bind(workspace)
        .fetch_optional(&mut *tx)
        .await
        .map_err(query("hold workspace while accounting verified storage"))?;
    if parent.is_some() {
        super::super::lock_storage(&mut tx, workspace).await?;
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
