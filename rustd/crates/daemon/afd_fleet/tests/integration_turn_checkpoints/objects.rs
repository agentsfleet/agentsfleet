//! Registration admits distinct file chunks within the capture's budget, and a
//! transfer names each registered chunk once under the ciphertext it froze.

use af_core::error_code;
use af_wire::{
    durable_task::{
        CheckpointObject, CheckpointPlaintext, CheckpointTransferPurpose, CheckpointUpload,
    },
    run::{
        CheckpointOperationId, TurnCheckpointAction as Action, TurnCheckpointAuthorize,
        TurnCheckpointObjects, TurnCheckpointRef, TurnCheckpointResponse as Response,
        TurnCheckpointUploads, TurnWorkspace,
    },
};

use super::{
    Checked,
    custody::{code, reader},
    files::{CONTENT, begin, upload_bytes},
    manifest::Pending,
    storage::{self, Files},
};

const OTHER: &[u8] = b"a second private chunk";

fn plaintext(bytes: &[u8], kind: &str) -> Checked<CheckpointPlaintext> {
    Ok(CheckpointPlaintext {
        sha256: af_core::etag::sha256_hex(bytes),
        size: u64::try_from(bytes.len())?,
        kind: kind.into(),
    })
}

async fn register(
    files: &Files,
    reference: &TurnCheckpointRef,
    objects: Vec<CheckpointPlaintext>,
) -> Checked<Vec<CheckpointObject>> {
    let Response::Objects(registered) = files
        .execute(Action::Objects(TurnCheckpointObjects {
            checkpoint: reference.clone(),
            objects,
        }))
        .await?
    else {
        return Err("registered objects".into());
    };
    Ok(registered)
}

async fn writer(files: &Files) -> Checked<(af_wire::run::TurnCheckpointPlan, TurnCheckpointRef)> {
    let (plan, _) = begin(files).await?;
    let reference = TurnCheckpointRef {
        checkpoint_id: plan.checkpoint_id.clone(),
        pin_id: CheckpointOperationId(plan.pin.id.clone()),
    };
    Ok((plan, reference))
}

// Value: protects=one registration names each chunk once, and only file
// chunks; fails_when=registration drops its duplicate-content or kind refusal;
// why_new=both arms had no case; seam=the production Objects over live
// Postgres and the private storage fixture.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn registration_refuses_repeated_content_and_a_non_file_chunk() -> Checked {
    let files = Files::new().await?;
    let (_, reference) = writer(&files).await?;
    for objects in [
        vec![plaintext(CONTENT, "file")?, plaintext(CONTENT, "file")?],
        vec![plaintext(CONTENT, "history")?],
    ] {
        assert_eq!(
            code(register(&files, &reference, objects).await),
            Some(error_code::RUN_CHECKPOINT_INCOMPATIBLE)
        );
    }
    assert_eq!(
        register(&files, &reference, vec![plaintext(CONTENT, "file")?])
            .await?
            .len(),
        1
    );
    files.finish().await;
    Ok(())
}

// Value: protects=a capture never registers past the upload budget it began
// with; fails_when=registration drops its budget check; why_new=the exhausted
// budget had no case; seam=the production Objects on a capture begun with no
// upload budget.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn registration_past_the_upload_budget_is_a_limit() -> Checked {
    let files = Files::new().await?;
    let pending = Pending::new(&files, &[Vec::new()]).await?;
    assert_eq!(
        code(
            register(
                &files,
                &pending.reference,
                vec![plaintext(CONTENT, "file")?]
            )
            .await
        ),
        Some(error_code::RUN_CHECKPOINT_LIMIT)
    );
    files.finish().await;
    Ok(())
}

// Value: protects=an upload signs only chunks registered to this capture, each
// once, and no authorization reads an upload back; fails_when=scope stops
// counting the chunks it found against the ones named, or an upload
// authorization is served again; why_new=a repeated chunk and the removed
// read-back had no case; seam=the production Uploads and Authorize over live
// Postgres.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_upload_naming_a_chunk_twice_conflicts_and_nothing_reads_one_back() -> Checked {
    let files = Files::new().await?;
    let (_, reference) = writer(&files).await?;
    let object = register(&files, &reference, vec![plaintext(CONTENT, "file")?])
        .await?
        .pop()
        .ok_or("object")?;
    let upload = CheckpointUpload {
        object_id: object.object_id.clone(),
        ciphertext_sha256: "a".repeat(64),
        ciphertext_length: u64::try_from(CONTENT.len() + af_crypto::TAG_LEN)?,
    };
    assert_eq!(
        code(
            files
                .execute(Action::Uploads(TurnCheckpointUploads {
                    checkpoint: reference.clone(),
                    objects: vec![upload.clone(), upload],
                }))
                .await
        ),
        Some(error_code::RUN_CHECKPOINT_CONFLICT)
    );
    assert_eq!(
        code(
            files
                .execute(Action::Authorize(TurnCheckpointAuthorize {
                    checkpoint: reference,
                    purpose: CheckpointTransferPurpose::Upload,
                    object_ids: vec![CheckpointOperationId(object.object_id)],
                }))
                .await
        ),
        Some(error_code::RUN_CHECKPOINT_INCOMPATIBLE)
    );
    files.finish().await;
    Ok(())
}

// Value: protects=an upload's ciphertext is frozen at its first descriptor;
// fails_when=freeze stops comparing a second descriptor's digest with the
// frozen one; why_new=the changed descriptor had no case; seam=the production
// Uploads twice for one chunk under two digests.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_upload_frozen_under_one_digest_refuses_another() -> Checked {
    let files = Files::new().await?;
    let (_, reference) = writer(&files).await?;
    let object = register(&files, &reference, vec![plaintext(CONTENT, "file")?])
        .await?
        .pop()
        .ok_or("object")?;
    let bytes = storage::encrypt(&files.case.acquired.workspace_id, &object, CONTENT)?;
    upload_bytes(&files, &reference, &object, bytes.clone()).await?;
    let mut altered = bytes;
    *altered.first_mut().ok_or("nonempty ciphertext")? ^= 1;
    assert_eq!(
        code(
            files
                .execute(Action::Uploads(TurnCheckpointUploads {
                    checkpoint: reference,
                    objects: vec![CheckpointUpload {
                        object_id: object.object_id,
                        ciphertext_sha256: af_core::etag::sha256_hex(&altered),
                        ciphertext_length: u64::try_from(altered.len())?,
                    }],
                }))
                .await
        ),
        Some(error_code::RUN_CHECKPOINT_CONFLICT)
    );
    files.finish().await;
    Ok(())
}

// Value: protects=a restorer is never handed a chunk the daemon did not
// authenticate; fails_when=signing a restore stops requiring a verified chunk;
// why_new=the unverified restore had no case; seam=the production publication
// of one verified chunk beside an unverified one, then a restore Authorize of
// the unverified chunk under a live reader pin.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_restore_never_signs_an_unverified_chunk() -> Checked {
    let files = Files::new().await?;
    let mut command = files.case.command();
    let history = super::publication::root(
        &files.case,
        TurnWorkspace::Files {
            toolbox_digest: "a".repeat(64),
        },
    );
    let Action::Begin(input) = &mut command.action else {
        return Err("begin command".into());
    };
    input.workspace = TurnWorkspace::Files {
        toolbox_digest: "a".repeat(64),
    };
    input.history_digest = af_core::etag::sha256_hex(history.history.as_bytes());
    input.page_count = 1;
    input.entry_count = 1;
    input.logical_bytes = u64::try_from(CONTENT.len())?;
    input.upload_budget_bytes =
        u64::try_from(CONTENT.len() + OTHER.len() + 2 * af_crypto::TAG_LEN)?;
    let Response::Begun(plan) = files.execute(command.action).await? else {
        return Err("allocated plan".into());
    };
    let reference = TurnCheckpointRef {
        checkpoint_id: plan.checkpoint_id.clone(),
        pin_id: CheckpointOperationId(plan.pin.id.clone()),
    };
    let registered = register(
        &files,
        &reference,
        vec![plaintext(CONTENT, "file")?, plaintext(OTHER, "file")?],
    )
    .await?;
    let [kept, unverified] = registered.as_slice() else {
        return Err("two registered chunks".into());
    };
    let bytes = storage::encrypt(&files.case.acquired.workspace_id, kept, CONTENT)?;
    upload_bytes(&files, &reference, kept, bytes).await?;
    let saved = super::files::publish(&files, *plan, history, reference, &kept.object_id).await?;
    let held = reader(&files, &saved).await?;
    assert_eq!(
        code(
            files
                .execute(Action::Authorize(TurnCheckpointAuthorize {
                    checkpoint: held,
                    purpose: CheckpointTransferPurpose::Restore,
                    object_ids: vec![CheckpointOperationId(unverified.object_id.clone())],
                }))
                .await
        ),
        Some(error_code::RUN_CHECKPOINT_INCOMPATIBLE)
    );
    files.finish().await;
    Ok(())
}
