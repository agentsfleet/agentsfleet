//! One vault key per workspace seals every checkpoint, under the cipher the deployment names.

use super::{
    Case, Checked,
    key_fixture::{frame_key, held, recorded, restore, save, unusable},
    storage::Files,
};
use af_core::test_util::trace::Capture;
use af_crypto::checkpoint::{Aes256Gcm, CheckpointScope, ChunkCipher as _};
use af_wire::durable_task::{CheckpointCipher, CheckpointObjectStatus};
use afd_vault::{CHECKPOINT_KEY_NAME, SecretBody, SecretName};
use serde_json::value::RawValue;
use std::sync::Arc;
use zeroize::Zeroizing;

const FIRST: &[u8] = b"the first generation's workspace file";
const SECOND: &[u8] = b"a file the second generation adds";
/// TEST-CONTEND: an exactly-once claim is proven by at least 100 callers.
const FIRST_SAVES: usize = 100;

// Value: protects=a replacement runner restores what a dead runner sealed, with
// the workspace's key reaching it only in that workspace's authorization
// frames; fails_when=a frame carries a per-object or per-runner key, or another
// workspace's frame carries this key; seam=TurnCheckpoints::execute over live
// Postgres and the S3 fixture, the run reassigned to a second runner.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_restore_on_another_runner_opens_chunks_under_the_workspace_key() -> Checked {
    let mut files = Files::new().await?;
    let workspace = files.case.acquired.workspace_id.clone();
    let (first, sealed_first) = save(&files, &[FIRST], None).await?;
    let (_second, sealed_second) = save(&files, &[FIRST, SECOND], Some(&first)).await?;
    let saved_under = frame_key(sealed_first.first().ok_or("first frame")?)?;
    assert_eq!(saved_under.len(), af_crypto::KEY_LEN);
    for frame in sealed_first.iter().chain(&sealed_second) {
        assert_eq!(frame_key(frame)?, saved_under, "one key seals every chunk");
    }
    assert_eq!(held(&files.case, &workspace).await?.0, 1);

    files.case.replace().await?;
    let restored = restore(&files).await?.ok_or("the head is offered")?;
    let contents: Vec<&[u8]> = restored.iter().map(|(_, bytes)| bytes.as_slice()).collect();
    assert_eq!(contents.len(), 2);
    assert!(contents.contains(&FIRST) && contents.contains(&SECOND));
    for (frame, _plaintext) in &restored {
        assert_eq!(frame_key(frame)?, saved_under, "the daemon hands B the key");
        assert_eq!(frame.cipher, CheckpointCipher::Aes256Gcm);
    }

    let other = Files::new().await?;
    assert_ne!(other.case.acquired.workspace_id, workspace);
    let (_saved, foreign) = save(&other, &[FIRST], None).await?;
    let foreign = foreign.first().ok_or("another workspace's frame")?;
    assert_ne!(frame_key(foreign)?, saved_under);
    assert!(matches!(foreign.status, CheckpointObjectStatus::Missing));
    other.finish().await;
    files.finish().await;
    Ok(())
}

/// Every caller's sealed chunk, each sealed under the key its own first save read.
type Sealed = (usize, [u8; 12], Vec<u8>);

/// Releases `FIRST_SAVES` first saves of one workspace from a barrier; each
/// seals a chunk under the key it was given.
async fn first_saves(
    keys: &afd_fleet::durable_task::storage::CheckpointKeys,
    workspace: &af_core::id::Uuid7,
    object: &af_core::id::Uuid7,
) -> Checked<Vec<Sealed>> {
    let barrier = Arc::new(tokio::sync::Barrier::new(FIRST_SAVES));
    let mut saves = tokio::task::JoinSet::new();
    for caller in 0..FIRST_SAVES {
        let (keys, barrier) = (keys.clone(), Arc::clone(&barrier));
        let (workspace, object) = (workspace.clone(), object.clone());
        saves.spawn(async move {
            barrier.wait().await;
            let key = keys
                .save_key(&workspace)
                .await
                .map_err(|fault| fault.to_string())?;
            let nonce = [u8::try_from(caller).map_err(|fault| fault.to_string())?; 12];
            let scope = CheckpointScope {
                workspace_id: &workspace,
                repository: "turn:first-saves",
                object_id: &object,
                format_version: 1,
            };
            let chunk = Zeroizing::new(caller.to_be_bytes().to_vec());
            Aes256Gcm
                .seal(&key, &nonce, &scope.binding(), chunk)
                .map(|sealed| (caller, nonce, sealed))
                .map_err(|fault| fault.to_string())
        });
    }
    let mut sealed = Vec::new();
    while let Some(save) = saves.join_next().await {
        sealed.push(save??);
    }
    Ok(sealed)
}

// Value: protects=a workspace has one checkpoint key however many saves race to
// be its first; fails_when=the create stops refusing a held name, or a loser
// seals under bytes it minted itself; seam=the save path's key read
// (`CheckpointKeys::save_key`) over live Postgres, released from a barrier.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn concurrent_first_saves_create_one_checkpoint_secret() -> Checked {
    let case = Case::new().await?;
    let other = Case::new().await?;
    let workspace = case.acquired.workspace_id.clone();
    let keys = super::keys_under(&case.fixtures, CheckpointCipher::default())?;
    let object = afd_db::test_util::mint_id();
    let capture = Capture::install();
    let sealed = first_saves(&keys, &workspace, &object).await?;
    let created = capture
        .events()
        .iter()
        .filter(|event| event.field("event") == Some("checkpoint_secret_created"))
        .filter(|event| event.field("workspace_id") == Some(workspace.as_str()))
        .count();
    drop(capture);
    assert_eq!(sealed.len(), FIRST_SAVES);
    assert_eq!(created, 1, "one first save creates the row");
    assert_eq!(held(&case, &workspace).await?.0, 1);

    let key = keys.save_key(&workspace).await?;
    let scope = CheckpointScope {
        workspace_id: &workspace,
        repository: "turn:first-saves",
        object_id: &object,
        format_version: 1,
    };
    for (caller, nonce, chunk) in sealed {
        let opened = Aes256Gcm.open(&key, &nonce, &scope.binding(), chunk)?;
        assert_eq!(opened.expose(), caller.to_be_bytes());
    }

    let foreign = keys.save_key(&other.acquired.workspace_id).await?;
    assert_ne!(foreign.expose(), key.expose());
    assert_eq!(held(&other, &other.acquired.workspace_id).await?.0, 1);
    other.fixtures.cleanup().await;
    case.fixtures.cleanup().await;
    Ok(())
}

/// The row replaced behind the daemon's back: the head's chunks were not sealed
/// by the key now held, so the restore is offered nothing.
async fn replaced_behind_the_daemon(files: &mut Files, capture: &Capture) -> Checked {
    let workspace = files.case.acquired.workspace_id.clone();
    let replacement = RawValue::from_string(format!(r#"{{"key":"{}"}}"#, "5a".repeat(32)))?;
    let vault = files.case.fixtures.plane().vault;
    vault
        .replace(
            &workspace,
            &SecretName::parse(CHECKPOINT_KEY_NAME)?,
            &SecretBody::parse(&replacement)?,
        )
        .await?;
    files.case.replace().await?;
    assert!(
        restore(files).await?.is_none(),
        "a replaced key reads nothing"
    );
    assert_eq!(unusable(capture, "replaced"), 1);
    assert_eq!(
        held(&files.case, &workspace).await?.1,
        0,
        "its generations are retired"
    );
    Ok(())
}

/// The row deleted: a restore of what the lost key sealed is offered nothing,
/// and every row that key sealed is retired.
async fn deleted_from_the_vault(files: &mut Files, capture: &Capture) -> Checked {
    let workspace = files.case.acquired.workspace_id.clone();
    let (_saved, _sealed) = save(files, &[SECOND], None).await?;
    let mut connection = files.case.fixtures.database.acquire().await?;
    sqlx::query("DELETE FROM vault.secrets WHERE workspace_id=$1 AND key_name=$2")
        .bind(&workspace)
        .bind(CHECKPOINT_KEY_NAME)
        .execute(&mut *connection)
        .await?;
    drop(connection);
    files.case.replace().await?;
    assert!(restore(files).await?.is_none(), "a lost key reads nothing");
    assert_eq!(unusable(capture, "absent"), 1);
    let (keys_held, generations, objects, _owed) = held(&files.case, &workspace).await?;
    assert_eq!((keys_held, generations, objects), (0, 0, 0));
    Ok(())
}

// Value: protects=a rotated, replaced or deleted key never serves a byte of
// what the old key sealed, and the run starts from its message;
// fails_when=rotation leaves a generation or an object row behind, the next
// save reuses an old object, or a restore is offered a head its key did not
// seal; seam=CheckpointKeys::rotate and TurnCheckpoints::resume over live
// Postgres and the S3 fixture.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_rotated_or_lost_key_starts_the_run_from_its_message() -> Checked {
    let mut files = Files::new().await?;
    let workspace = files.case.acquired.workspace_id.clone();
    let keys = files.store.keys().clone();
    let (_saved, sealed) = save(&files, &[FIRST], None).await?;
    let old = frame_key(sealed.first().ok_or("frame")?)?;
    assert_eq!(held(&files.case, &workspace).await?, (1, 1, 1, 0));

    let retired = keys
        .rotate(&workspace)
        .await?
        .ok_or("the workspace exists")?;
    assert_eq!((retired.generations, retired.objects), (1, 1));
    assert_eq!(held(&files.case, &workspace).await?, (1, 0, 0, 1));
    assert_eq!(keys.rotate(&afd_db::test_util::mint_id()).await?, None);
    files.case.replace().await?;
    assert!(restore(&files).await?.is_none(), "no head after a rotation");
    let (_saved, resealed) = save(&files, &[FIRST], None).await?;
    let resealed = resealed.first().ok_or("frame")?;
    assert!(
        matches!(resealed.status, CheckpointObjectStatus::Missing),
        "the save after a rotation uploads everything again"
    );
    assert_ne!(frame_key(resealed)?, old);

    let capture = Capture::install();
    replaced_behind_the_daemon(&mut files, &capture).await?;
    deleted_from_the_vault(&mut files, &capture).await?;
    drop(capture);
    files.finish().await;
    Ok(())
}

// Value: protects=a checkpoint restores under the cipher that sealed it after
// the deployment's setting changes, in both directions; fails_when=a restore
// opens with the configured cipher instead of the recorded one, or a plain
// deployment stops saying so at boot; seam=two `[checkpoint] cipher` settings
// over one bucket, vault and schema.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_checkpoint_opens_under_the_cipher_that_sealed_it() -> Checked {
    let case = Case::new().await?;
    let capture = Capture::install();
    let mut files = Files::sealing(case, CheckpointCipher::Plain).await?;
    let announced = announced_plain(&capture);
    assert_eq!(announced, 1, "a plain deployment says so once at boot");
    let workspace = files.case.acquired.workspace_id.clone();

    let (plain, sealed) = save(&files, &[FIRST], None).await?;
    assert_eq!(
        sealed.first().map(|frame| frame.cipher),
        Some(CheckpointCipher::Plain)
    );
    assert_eq!(recorded(&files.case, &workspace).await?, ["plain"]);
    files.restart_sealing(CheckpointCipher::Aes256Gcm)?;
    files.case.replace().await?;
    let restored = restore(&files).await?.ok_or("the plain head is offered")?;
    let (frame, plaintext) = restored.first().ok_or("the plain chunk")?;
    assert_eq!(
        (frame.cipher, plaintext.as_slice()),
        (CheckpointCipher::Plain, FIRST)
    );
    assert_eq!(frame.ciphertext_length, Some(u64::try_from(FIRST.len())?));

    let (_encrypted, sealed) = save(&files, &[SECOND], Some(&plain)).await?;
    let frame = sealed.first().ok_or("the encrypted chunk")?;
    assert_eq!(frame.cipher, CheckpointCipher::Aes256Gcm);
    assert_eq!(
        recorded(&files.case, &workspace).await?,
        ["plain", "aes_256_gcm"]
    );
    files.restart_sealing(CheckpointCipher::Plain)?;
    files.case.replace().await?;
    let restored = restore(&files)
        .await?
        .ok_or("the encrypted head is offered")?;
    let (frame, plaintext) = restored.first().ok_or("the encrypted chunk")?;
    assert_eq!(
        (frame.cipher, plaintext.as_slice()),
        (CheckpointCipher::Aes256Gcm, SECOND)
    );
    assert_eq!(
        frame.ciphertext_length,
        Some(u64::try_from(SECOND.len() + af_crypto::TAG_LEN)?)
    );
    let announced = announced_plain(&capture);
    assert_eq!(announced, 2, "each boot under plain says so once");
    files.finish().await;
    Ok(())
}

/// How many times a boot said the deployment stores checkpoints plain.
fn announced_plain(capture: &Capture) -> usize {
    capture
        .events()
        .iter()
        .filter(|event| event.field("event") == Some("checkpoint_cipher_plain"))
        .count()
}
