//! What the workspace-key cases share: saving a generation as a runner does,
//! restoring as a replacement does, and counting the rows a workspace holds.

use super::{
    Case, Checked, files,
    publication::root,
    storage::{self, Files},
};
use af_core::{id::Uuid7, test_util::trace::Capture};
use af_crypto::{checkpoint::CheckpointScope, secret::SecretBytes};
use af_wire::{
    durable_task::{
        CheckpointEntry, CheckpointFile, CheckpointObject, CheckpointObjectStatus, CheckpointPage,
        CheckpointPlaintext, CheckpointTransferPurpose,
    },
    run::{
        CheckpointOperationId, TurnCheckpointAction as Action, TurnCheckpointAuthorize,
        TurnCheckpointCommit, TurnCheckpointObjects, TurnCheckpointPageRead,
        TurnCheckpointPageWrite, TurnCheckpointPinRelease, TurnCheckpointRead, TurnCheckpointRef,
        TurnCheckpointResponse as Response, TurnResume, TurnWorkspace,
    },
};
use afd_fleet::durable_task::storage::chunk_cipher;
use afd_vault::CHECKPOINT_KEY_NAME;
use base64::{Engine as _, prelude::BASE64_STANDARD};

pub(super) const EVENT_UNUSABLE: &str = "checkpoint_key_unusable";

/// One generation holding a file per `contents`, saved over `head`. Content
/// the turn already stores is reused; the rest is sealed with the frame's key
/// and cipher, and uploaded.
pub(super) async fn save(
    files: &Files,
    contents: &[&[u8]],
    head: Option<&TurnResume>,
) -> Checked<(TurnResume, Vec<CheckpointObject>)> {
    let (plan, history) = begun(files, contents, head).await?;
    let reference = TurnCheckpointRef {
        checkpoint_id: plan.checkpoint_id.clone(),
        pin_id: CheckpointOperationId(plan.pin.id.clone()),
    };
    let (objects, uploaded) = registered(files, &reference, contents).await?;
    let page_id = plan.page_ids.first().ok_or("page")?.clone();
    let page = page_of(contents, &objects)?;
    let digest = af_core::etag::sha256_hex(&serde_json::to_vec(&page)?);
    let written = Action::PageWrite(TurnCheckpointPageWrite {
        checkpoint: reference.clone(),
        page_id: page_id.clone(),
        digest: digest.clone(),
        contents: page,
    });
    assert!(matches!(
        files.execute(written).await?,
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
            uploaded,
        }))
        .await?
    else {
        return Err("committed generation".into());
    };
    Ok((*saved, objects))
}

/// A file plan for `contents` over `head`, and the history root it commits.
pub(super) async fn begun(
    files: &Files,
    contents: &[&[u8]],
    head: Option<&TurnResume>,
) -> Checked<(
    af_wire::run::TurnCheckpointPlan,
    af_wire::run::TurnPrivateRoot,
)> {
    let workspace = TurnWorkspace::Files {
        toolbox_digest: "a".repeat(64),
    };
    let history = root(&files.case, workspace.clone());
    let bytes: usize = contents.iter().map(|content| content.len()).sum();
    let mut command = files.case.command();
    let Action::Begin(begin) = &mut command.action else {
        return Err("begin command".into());
    };
    begin.expected_head = head.map(|head| head.checkpoint_id.clone());
    begin.workspace = workspace;
    begin.history_digest = af_core::etag::sha256_hex(history.history.as_bytes());
    begin.page_count = 1;
    begin.entry_count = u32::try_from(contents.len())?;
    begin.logical_bytes = u64::try_from(bytes)?;
    begin.upload_budget_bytes = u64::try_from(bytes + contents.len() * af_crypto::TAG_LEN)?;
    let Response::Begun(plan) = files.execute(command.action).await? else {
        return Err("allocated plan".into());
    };
    Ok((*plan, history))
}

/// One manifest page listing a file per content, each held by its object.
pub(super) fn page_of(contents: &[&[u8]], objects: &[CheckpointObject]) -> Checked<CheckpointPage> {
    let mut entries = Vec::new();
    for (index, (content, object)) in contents.iter().zip(objects).enumerate() {
        entries.push(CheckpointEntry::File(CheckpointFile {
            path: format!("state-{index}.txt"),
            size: u64::try_from(content.len())?,
            executable: false,
            objects: vec![object.object_id.clone()],
        }));
    }
    Ok(CheckpointPage {
        page_number: 0,
        entries,
        history_objects: Vec::new(),
    })
}

/// Registers `contents` and uploads whichever the turn does not store yet.
/// Each returned frame is the registration's, with its key and cipher,
/// beside the objects whose PUT was accepted.
pub(super) async fn registered(
    files: &Files,
    reference: &TurnCheckpointRef,
    contents: &[&[u8]],
) -> Checked<(Vec<CheckpointObject>, Vec<CheckpointOperationId>)> {
    let descriptors = contents
        .iter()
        .map(|content| {
            Ok(CheckpointPlaintext {
                sha256: af_core::etag::sha256_hex(content),
                size: u64::try_from(content.len())?,
                kind: "file".into(),
            })
        })
        .collect::<Checked<Vec<_>>>()?;
    let Response::Objects(objects) = files
        .execute(Action::Objects(TurnCheckpointObjects {
            checkpoint: reference.clone(),
            objects: descriptors,
        }))
        .await?
    else {
        return Err("registered objects".into());
    };
    let mut uploaded = Vec::new();
    for (content, object) in contents.iter().zip(&objects) {
        if matches!(object.status, CheckpointObjectStatus::Missing) {
            let sealed = storage::encrypt(&files.case.acquired.workspace_id, object, content)?;
            files::upload_bytes(files, reference, object, sealed).await?;
            uploaded.push(CheckpointOperationId(object.object_id.clone()));
        }
    }
    Ok((objects, uploaded))
}

/// What the current run restores: nothing when it is offered no checkpoint,
/// else each chunk's restore frame beside the plaintext it opened to.
pub(super) async fn restore(files: &Files) -> Checked<Option<Vec<(CheckpointObject, Vec<u8>)>>> {
    let offered = files
        .store
        .resume(&files.case.runs, files.case.run())
        .await?;
    let Some(descriptor) = offered else {
        return Ok(None);
    };
    let Response::Read(saved) = files
        .execute(Action::Read(TurnCheckpointRead {
            descriptor: Box::new(descriptor.clone()),
        }))
        .await?
    else {
        return Err("reader custody".into());
    };
    let reference = TurnCheckpointRef {
        checkpoint_id: descriptor.checkpoint_id,
        pin_id: CheckpointOperationId(saved.pin.id),
    };
    let page_id = saved.page_ids.first().ok_or("reader page")?.clone();
    let frames = frames(files, &reference, page_id).await?;
    let mut opened = Vec::new();
    for frame in frames {
        let url = &frame.transfer.as_ref().ok_or("download link")?.url;
        let stored = files.server.client.get(url).send().await?;
        let stored = stored.error_for_status()?.bytes().await?.to_vec();
        let plaintext = open(&files.case.acquired.workspace_id, &frame, stored)?;
        opened.push((frame, plaintext));
    }
    let released = Action::PinRelease(TurnCheckpointPinRelease {
        checkpoint: reference,
    });
    assert!(matches!(
        files.execute(released).await?,
        Response::PinReleased
    ));
    Ok(Some(opened))
}

/// The restore frames of every file object the head's page lists.
async fn frames(
    files: &Files,
    reference: &TurnCheckpointRef,
    page_id: CheckpointOperationId,
) -> Checked<Vec<CheckpointObject>> {
    let Response::PageRead(page) = files
        .execute(Action::PageRead(TurnCheckpointPageRead {
            checkpoint: reference.clone(),
            page_id,
        }))
        .await?
    else {
        return Err("reader page".into());
    };
    let ids = page
        .entries
        .iter()
        .filter_map(|entry| match entry {
            CheckpointEntry::File(file) => Some(file.objects.clone()),
            _other => None,
        })
        .flatten()
        .map(CheckpointOperationId)
        .collect();
    let Response::Authorized(frames) = files
        .execute(Action::Authorize(TurnCheckpointAuthorize {
            checkpoint: reference.clone(),
            purpose: CheckpointTransferPurpose::Restore,
            object_ids: ids,
        }))
        .await?
    else {
        return Err("restore frames".into());
    };
    Ok(frames)
}

pub(super) fn frame_key(frame: &CheckpointObject) -> Checked<Vec<u8>> {
    Ok(BASE64_STANDARD.decode(frame.data_key.as_deref().ok_or("the frame's key")?)?)
}

/// Opens stored bytes as the supervisor does: the frame's key, nonce and cipher.
fn open(workspace: &Uuid7, frame: &CheckpointObject, stored: Vec<u8>) -> Checked<Vec<u8>> {
    let nonce = BASE64_STANDARD.decode(frame.nonce.as_deref().ok_or("nonce")?)?;
    let nonce: [u8; af_crypto::NONCE_LEN] = nonce.as_slice().try_into()?;
    let id = Uuid7::parse(&frame.object_id)?;
    let scope = CheckpointScope {
        workspace_id: workspace,
        repository: &frame.repository,
        object_id: &id,
        format_version: 1,
    };
    let key = SecretBytes::new(frame_key(frame)?);
    let plaintext = chunk_cipher(frame.cipher).open(&key, &nonce, &scope.binding(), stored)?;
    Ok(plaintext.expose().to_vec())
}

/// The rows a workspace holds: its key rows, generations, object rows and owed bucket deletes.
pub(super) async fn held(case: &Case, workspace: &Uuid7) -> Checked<(i64, i64, i64, i64)> {
    let mut connection = case.fixtures.database.acquire().await?;
    Ok(sqlx::query_as(
        "SELECT
        (SELECT count(*) FROM vault.secrets WHERE workspace_id=$1 AND key_name=$2),
        (SELECT count(*) FROM runner.checkpoints WHERE workspace_id=$1),
        (SELECT count(*) FROM runner.checkpoint_objects WHERE workspace_id=$1),
        (SELECT count(*) FROM runner.checkpoint_purges WHERE workspace_id=$1 AND completed_at IS NULL)",
    )
    .bind(workspace)
    .bind(CHECKPOINT_KEY_NAME)
    .fetch_one(&mut *connection)
    .await?)
}

pub(super) fn unusable(capture: &Capture, fault: &str) -> usize {
    capture
        .events()
        .iter()
        .filter(|event| {
            event.field("event") == Some(EVENT_UNUSABLE) && event.field("fault") == Some(fault)
        })
        .count()
}

/// The cipher each of the workspace's generations recorded, oldest first.
pub(super) async fn recorded(case: &Case, workspace: &Uuid7) -> Checked<Vec<String>> {
    let mut connection = case.fixtures.database.acquire().await?;
    Ok(sqlx::query_scalar(
        "SELECT cipher FROM runner.checkpoints WHERE workspace_id=$1 ORDER BY generation",
    )
    .bind(workspace)
    .fetch_all(&mut *connection)
    .await?)
}

/// The fixtures' checkpoint keys under `cipher`: one vault, the fixture master key.
pub(super) fn keys_under(
    fixtures: &crate::support::Fixtures,
    cipher: af_wire::durable_task::CheckpointCipher,
) -> Checked<afd_fleet::durable_task::storage::CheckpointKeys> {
    Ok(afd_fleet::durable_task::storage::CheckpointKeys::over(
        fixtures.database.clone(),
        std::sync::Arc::new(af_crypto::secret::Kek::from_hex(
            crate::report_seed::FIXTURE_KEK_HEX,
        )?),
        cipher,
    ))
}

/// A turn checkpoint store with no file backend, sealing with the default cipher.
pub(super) fn turn_store(case: &Case) -> Checked<afd_fleet::run::turn_checkpoint::TurnCheckpoints> {
    Ok(afd_fleet::run::turn_checkpoint::TurnCheckpoints::new(
        keys_under(
            &case.fixtures,
            af_wire::durable_task::CheckpointCipher::default(),
        )?,
    ))
}
