//! Authenticated bytes cross the real native, TLS, crypto and filesystem boundaries.

use super::*;
use crate::{
    client::Verb,
    recovery::{progress::Progress, transfer, transfer::http_tests::server::Server},
    test_support::{self, Answer},
};
use af_wire::task::{
    CheckpointAuthorizations, CheckpointFile, CheckpointObject, CheckpointObjectStatus,
    CheckpointPlaintext, CheckpointTransfer,
};
use base64::Engine as _;
use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
use tokio_util::sync::CancellationToken;

type TestResult<T = ()> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
const ID: &str = "019f0000-0000-7000-8000-000000000001";
const OBJECT: &str = "019f0000-0000-7000-8000-000000000010";
const CONTENT: &[u8] = b"#!/bin/sh\nexit 0\n";

fn encrypted(kind: &str) -> TestResult<(CheckpointObject, Vec<u8>)> {
    let mut object = CheckpointObject {
        object_id: OBJECT.to_owned(),
        repository: "owner/repo".to_owned(),
        plaintext: CheckpointPlaintext {
            sha256: af_core::etag::sha256_hex(CONTENT),
            size: crate::byte_count(CONTENT.len()),
            kind: kind.to_owned(),
        },
        status: CheckpointObjectStatus::Reused,
        nonce: None,
        data_key: None,
        ciphertext_sha256: None,
        ciphertext_length: None,
        transfer: None,
    };
    material(&mut object, 7);
    let bytes = transfer::seal(
        &Uuid7::parse(ID)?,
        &mut object,
        Zeroizing::new(CONTENT.to_vec()),
    )?;
    material(&mut object, 7);
    object.ciphertext_sha256 = Some(af_core::etag::sha256_hex(&bytes));
    object.ciphertext_length = Some(crate::byte_count(bytes.len()));
    Ok((object, bytes))
}

fn material(object: &mut CheckpointObject, byte: u8) {
    object.nonce = Some(base64::prelude::BASE64_STANDARD.encode([4; af_crypto::NONCE_LEN]));
    object.data_key = Some(base64::prelude::BASE64_STANDARD.encode([byte; af_crypto::KEY_LEN]));
}

fn plan(object: &CheckpointObject) -> CheckpointPlan {
    let expires = af_core::clock::after(af_core::clock::now(), std::time::Duration::from_mins(10));
    CheckpointPlan {
        checkpoint_id: ID.to_owned(),
        generation: 1,
        base_id: None,
        mode: CheckpointMode::Incremental,
        repository: object.repository.clone(),
        format_version: CHECKPOINT_FORMAT,
        toolbox_digest: "a".repeat(64),
        page_ids: vec![],
        base_page_ids: vec![],
        base_manifest_root: None,
        base_commit_oid: None,
        manifest_root: None,
        history_digest: Some(af_crypto::checkpoint_digest::history_root([(
            OBJECT,
            object.plaintext.sha256.as_str(),
            object.plaintext.size,
        )])),
        covered_input_sequence: 3,
        effect_watermark: 2,
        expires_at: expires,
        pin: af_wire::task::CheckpointPin {
            id: ID.to_owned(),
            resource_version: 1,
            operation_deadline: expires,
            expires_at: expires,
        },
        replayed: false,
    }
}

fn native(
    object: CheckpointObject,
    plan: CheckpointPlan,
) -> TestResult<(ControlPlane, test_support::Calls)> {
    let response = serde_json::to_value(CheckpointAuthorizations {
        plan,
        objects: vec![object],
    })?;
    Ok(test_support::plane(move |call| match call.verb {
        Verb::Progress => test_support::json(&serde_json::json!({})),
        Verb::TaskCheckpointAuthorizations => {
            let input = call.body.as_ref().and_then(|bytes| {
                serde_json::from_slice::<CheckpointAuthorizationInput>(bytes).ok()
            });
            assert!(input.as_ref().is_some_and(|input| input.fencing_token == 31
                && input.object_ids == [OBJECT]
                && input.pin_id.as_deref() == Some(ID)
                && input.purpose == CheckpointTransferPurpose::Restore));
            test_support::json(&response)
        }
        _ => Answer::Fail(error::config("unexpected object recovery request")),
    }))
}

fn session<'a>(
    plane: &'a ControlPlane,
    identity: &'a Uuid7,
    plan: CheckpointPlan,
    progress: &'a mut Progress,
    stop: &'a CancellationToken,
    server: &Server,
) -> Session<'a> {
    Session {
        plane,
        run: identity,
        workspace: identity,
        fence: 31,
        task: DecisionTaskVersion {
            task_id: ID.to_owned(),
            generation: 7,
        },
        plan,
        activity_id: ID.to_owned(),
        progress,
        transfers: server.transfers(),
        stop,
    }
}

async fn server_object(kind: &str, valid: bool) -> TestResult<(Server, CheckpointObject)> {
    let (mut object, bytes) = encrypted(kind)?;
    let server = Server::start(http::StatusCode::OK, bytes, false).await?;
    object.transfer = Some(CheckpointTransfer {
        url: server.url.clone(),
        headers: BTreeMap::new(),
        expires_at: af_core::clock::after(
            af_core::clock::now(),
            std::time::Duration::from_secs(30),
        ),
    });
    if !valid {
        material(&mut object, 8);
    }
    Ok((server, object))
}

#[tokio::test]
async fn repeated_manifest_ids_share_one_authorization_and_one_authenticated_download() -> TestResult
{
    let (mut server, object) = server_object("history", true).await?;
    let plan = plan(&object);
    let (plane, mut calls) = native(object, plan.clone())?;
    let identity = Uuid7::parse(ID)?;
    let mut progress = Progress::default();
    let stop = CancellationToken::new();
    let mut session = session(&plane, &identity, plan, &mut progress, &stop, &server);
    let bytes = session
        .download_batch(&[OBJECT.to_owned(), OBJECT.to_owned()])
        .await?;
    assert_eq!(bytes.len(), 2);
    assert!(bytes.iter().all(|bytes| bytes.expose() == CONTENT));
    assert_eq!(session.progress.completed, 1);
    assert_eq!(
        session.progress.bytes.acknowledged,
        crate::byte_count(CONTENT.len() + af_crypto::TAG_LEN)
    );
    assert_eq!(session.progress.bytes.missing, 0);
    let seen = server.seen.recv().await.ok_or("download missing")?;
    assert_eq!(seen.method, http::Method::GET);
    assert!(seen.headers.get("authorization").is_none());
    assert!(seen.headers.get("cookie").is_none());
    server.finish().await?;
    assert_eq!(
        test_support::drain(&mut calls)
            .iter()
            .filter(|call| call.verb == Verb::TaskCheckpointAuthorizations)
            .count(),
        1
    );
    Ok(())
}

#[tokio::test]
async fn nonempty_history_is_decrypted_and_checked_against_its_manifest_root() -> TestResult {
    for correct_root in [true, false] {
        let (server, object) = server_object("history", true).await?;
        let mut plan = plan(&object);
        if !correct_root {
            plan.history_digest = Some("f".repeat(64));
        }
        let (plane, _calls) = native(object, plan.clone())?;
        let identity = Uuid7::parse(ID)?;
        let mut progress = Progress::default();
        let stop = CancellationToken::new();
        let mut session = session(&plane, &identity, plan, &mut progress, &stop, &server);
        let result = session.history(vec![OBJECT.to_owned()]).await;
        if correct_root {
            assert_eq!(result?.expose(), CONTENT);
        } else {
            let Err(failure) = result else {
                return Err("changed history root accepted".into());
            };
            assert!(matches!(failure.kind(), error::ErrorKind::Config { detail }
                if *detail == "checkpoint history root mismatch"));
        }
        server.finish().await?;
    }
    Ok(())
}

#[tokio::test]
async fn nonempty_file_restore_preserves_exact_bytes_and_owner_only_executable_mode() -> TestResult
{
    let (server, object) = server_object("file", true).await?;
    let plan = plan(&object);
    let (plane, _calls) = native(object, plan.clone())?;
    let identity = Uuid7::parse(ID)?;
    let mut progress = Progress::default();
    let stop = CancellationToken::new();
    let mut session = session(&plane, &identity, plan, &mut progress, &stop, &server);
    let root = tempfile::tempdir()?;
    let metadata = root.path().metadata()?;
    let entries = BTreeMap::from([
        (
            "repo".to_owned(),
            CheckpointEntry::Directory {
                path: "repo".to_owned(),
            },
        ),
        (
            "repo/run.sh".to_owned(),
            CheckpointEntry::File(CheckpointFile {
                path: "repo/run.sh".to_owned(),
                executable: true,
                size: crate::byte_count(CONTENT.len()),
                objects: vec![OBJECT.to_owned()],
            }),
        ),
    ]);
    session
        .restore_files(
            Some((root.path().to_owned(), (metadata.uid(), metadata.gid()))),
            entries,
            None,
        )
        .await?;
    assert_eq!(std::fs::read(root.path().join("repo/run.sh"))?, CONTENT);
    let restored = std::fs::metadata(root.path().join("repo/run.sh"))?;
    assert_eq!(restored.permissions().mode() & 0o777, 0o700);
    assert_eq!(
        (restored.uid(), restored.gid()),
        (metadata.uid(), metadata.gid())
    );
    server.finish().await?;
    Ok(())
}

#[tokio::test]
async fn matching_ciphertext_descriptor_does_not_bypass_a_failed_authentication_tag() -> TestResult
{
    let (server, object) = server_object("history", false).await?;
    let plan = plan(&object);
    let (plane, _calls) = native(object, plan.clone())?;
    let identity = Uuid7::parse(ID)?;
    let mut progress = Progress::default();
    let stop = CancellationToken::new();
    let mut session = session(&plane, &identity, plan, &mut progress, &stop, &server);
    let result = session.download_batch(&[OBJECT.to_owned()]).await;
    let Err(failure) = result else {
        return Err("unauthenticated ciphertext was accepted".into());
    };
    assert!(matches!(failure.kind(), error::ErrorKind::Crypto { .. }));
    assert_eq!(session.progress.completed, 0);
    assert_eq!(session.progress.bytes.acknowledged, 0);
    assert_eq!(
        session.progress.bytes.missing,
        crate::byte_count(CONTENT.len() + af_crypto::TAG_LEN)
    );
    server.finish().await?;
    Ok(())
}
