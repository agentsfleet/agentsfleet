//! Matching event-owned files and root publish once every chunk's bound PUT is accepted.

use super::{
    Checked,
    publication::root,
    storage::{self, Files},
};
use af_wire::{
    durable_task::{
        CheckpointEntry, CheckpointFile, CheckpointPage, CheckpointPlaintext,
        CheckpointTransferPurpose, CheckpointUpload,
    },
    run::{
        CheckpointOperationId, TurnCheckpointAction as Action, TurnCheckpointAuthorize,
        TurnCheckpointCommit, TurnCheckpointObjects, TurnCheckpointPageRead,
        TurnCheckpointPageWrite, TurnCheckpointPinRelease, TurnCheckpointPlan, TurnCheckpointRead,
        TurnCheckpointRef, TurnCheckpointResponse as Response, TurnCheckpointUploads,
        TurnPrivateRoot, TurnResume, TurnWorkspace,
    },
};
use sqlx::Connection as _;
use std::time::Duration;

pub(super) const CONTENT: &[u8] = b"private accepted workspace state";

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn ordinary_files_and_root_publish_under_real_private_sdk_custody() -> Checked {
    let files = Files::new().await?;
    let (plan, history) = begin(&files).await?;
    let reference = TurnCheckpointRef {
        checkpoint_id: plan.checkpoint_id.clone(),
        pin_id: CheckpointOperationId(plan.pin.id.clone()),
    };
    let object = uploaded_chunk(&files, &reference).await?;
    let descriptor = publish(&files, plan, history, reference, &object.object_id).await?;
    read(
        &files,
        descriptor,
        &object.ciphertext_sha256.ok_or("frozen digest")?,
    )
    .await?;
    files.finish().await;
    Ok(())
}

pub(super) async fn begin(files: &Files) -> Checked<(TurnCheckpointPlan, TurnPrivateRoot)> {
    let workspace = TurnWorkspace::Files {
        toolbox_digest: "a".repeat(64),
        sandbox: super::sandbox(),
    };
    let history = root(&files.case, workspace.clone());
    let mut command = files.case.command();
    let Action::Begin(begin) = &mut command.action else {
        return Err("begin command".into());
    };
    begin.workspace = workspace;
    begin.history_digest = af_core::etag::sha256_hex(history.history.as_bytes());
    begin.page_count = 1;
    begin.entry_count = 1;
    begin.logical_bytes = u64::try_from(CONTENT.len())?;
    begin.upload_budget_bytes = u64::try_from(CONTENT.len() + af_crypto::TAG_LEN)?;
    let Response::Begun(plan) = files.execute(command.action).await? else {
        return Err("allocated plan".into());
    };
    Ok((*plan, history))
}

/// One registered chunk, its ciphertext frozen and PUT through its bound link.
pub(super) async fn uploaded_chunk(
    files: &Files,
    reference: &TurnCheckpointRef,
) -> Checked<af_wire::durable_task::CheckpointObject> {
    let Response::Objects(mut registered) = files
        .execute(Action::Objects(TurnCheckpointObjects {
            checkpoint: reference.clone(),
            objects: vec![CheckpointPlaintext {
                sha256: af_core::etag::sha256_hex(CONTENT),
                size: u64::try_from(CONTENT.len())?,
                kind: "file".into(),
            }],
        }))
        .await?
    else {
        return Err("registered object".into());
    };
    let object = registered.pop().ok_or("object")?;
    assert!(registered.is_empty());
    assert_eq!(
        object.repository,
        format!("turn:{}", files.case.acquired.turn_id)
    );
    let bytes = storage::encrypt(&files.case.acquired.workspace_id, &object, CONTENT)?;
    upload_bytes(files, reference, &object, bytes).await
}

pub(super) async fn publish(
    files: &Files,
    plan: TurnCheckpointPlan,
    history: TurnPrivateRoot,
    reference: TurnCheckpointRef,
    object: &str,
) -> Checked<TurnResume> {
    let page_id = plan.page_ids.first().ok_or("page")?.clone();
    let page = CheckpointPage {
        page_number: 0,
        entries: vec![CheckpointEntry::File(CheckpointFile {
            path: "state.txt".into(),
            size: u64::try_from(CONTENT.len())?,
            executable: true,
            objects: vec![object.into()],
        })],
        history_objects: Vec::new(),
    };
    let digest = af_core::etag::sha256_hex(&serde_json::to_vec(&page)?);
    assert!(matches!(
        files
            .execute(Action::PageWrite(TurnCheckpointPageWrite {
                checkpoint: reference.clone(),
                page_id: page_id.clone(),
                digest: digest.clone(),
                contents: page
            }))
            .await?,
        Response::PageWritten(_)
    ));
    let manifest =
        af_crypto::checkpoint_digest::manifest_root([(page_id.0.as_str(), digest.as_str())]);
    let Response::Committed(saved) = files
        .execute(Action::Commit(TurnCheckpointCommit {
            checkpoint: reference,
            root: history,
            manifest_root: manifest,
            folds: Vec::new(),
            uploaded: vec![CheckpointOperationId(object.into())],
        }))
        .await?
    else {
        return Err("committed files and root".into());
    };
    Ok(*saved)
}

async fn read(files: &Files, descriptor: TurnResume, digest: &str) -> Checked {
    let Response::Read(saved) = files
        .execute(Action::Read(TurnCheckpointRead {
            descriptor: Box::new(descriptor.clone()),
        }))
        .await?
    else {
        return Err("reader custody".into());
    };
    assert_eq!(saved.descriptor, descriptor);
    let reference = TurnCheckpointRef {
        checkpoint_id: descriptor.checkpoint_id,
        pin_id: CheckpointOperationId(saved.pin.id),
    };
    let page_id = saved.page_ids.first().ok_or("reader page")?.clone();
    let Response::PageRead(page) = files
        .execute(Action::PageRead(TurnCheckpointPageRead {
            checkpoint: reference.clone(),
            page_id,
        }))
        .await?
    else {
        return Err("reader page".into());
    };
    let Some(CheckpointEntry::File(file)) = page.entries.first() else {
        return Err("saved file".into());
    };
    assert!(file.executable);
    let Response::Authorized(mut objects) = authorize_with_storage_busy(
        files,
        Action::Authorize(TurnCheckpointAuthorize {
            checkpoint: reference.clone(),
            purpose: CheckpointTransferPurpose::Restore,
            object_ids: file
                .objects
                .iter()
                .cloned()
                .map(CheckpointOperationId)
                .collect(),
        }),
    )
    .await?
    else {
        return Err("reader object".into());
    };
    let object = objects.pop().ok_or("reader object")?;
    let transfer = object.transfer.ok_or("download transfer")?;
    download(files, transfer, digest).await?;
    release(files, reference).await?;
    Ok(())
}

async fn authorize_with_storage_busy(files: &Files, action: Action) -> Checked<Response> {
    let mut connection = files.case.fixtures.database.acquire().await?;
    let mut held = connection.begin().await?;
    sqlx::query("SELECT id FROM identity.lock_workspace_storage($1)")
        .bind(&files.case.acquired.workspace_id)
        .execute(&mut *held)
        .await?;
    let result = tokio::time::timeout(Duration::from_secs(10), files.execute(action)).await;
    held.rollback().await?;
    result.map_err(|_elapsed| "restore authorization stalled behind the workspace storage lock")?
}

async fn download(
    files: &Files,
    transfer: af_wire::durable_task::CheckpointTransfer,
    digest: &str,
) -> Checked {
    let bytes = files
        .server
        .client
        .get(transfer.url)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    assert_eq!(af_core::etag::sha256_hex(&bytes), digest);
    Ok(())
}

pub(super) async fn upload_bytes(
    files: &Files,
    reference: &TurnCheckpointRef,
    object: &af_wire::durable_task::CheckpointObject,
    bytes: Vec<u8>,
) -> Checked<af_wire::durable_task::CheckpointObject> {
    let upload = CheckpointUpload {
        object_id: object.object_id.clone(),
        ciphertext_sha256: af_core::etag::sha256_hex(&bytes),
        ciphertext_length: u64::try_from(bytes.len())?,
    };
    let Response::Uploads(mut authorized) = files
        .execute(Action::Uploads(TurnCheckpointUploads {
            checkpoint: reference.clone(),
            objects: vec![upload],
        }))
        .await?
    else {
        return Err("upload authorization".into());
    };
    let authorized = authorized.pop().ok_or("upload object")?;
    storage::upload(files, &authorized, bytes).await?;
    Ok(authorized)
}

async fn release(files: &Files, reference: TurnCheckpointRef) -> Checked {
    assert!(matches!(
        files
            .execute(Action::PinRelease(TurnCheckpointPinRelease {
                checkpoint: reference
            }))
            .await?,
        Response::PinReleased
    ));
    Ok(())
}
