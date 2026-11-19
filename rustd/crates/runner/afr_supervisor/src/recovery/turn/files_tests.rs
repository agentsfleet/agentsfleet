//! Ordinary file restore authenticates concurrent chunks and preserves their manifest order.

use super::{NativeTurn, session::TurnSession};
use crate::recovery::{transfer, transfer::Transfers, transfer::http_tests::server::Server};
use crate::{
    Result, error,
    test_support::{self, Answer},
};
use af_core::{clock, id::Uuid7};
use af_wire::{
    run::{
        TurnCheckpointAction as Action, TurnCheckpointCommand, TurnCheckpointRef,
        TurnCheckpointResponse as Response, TurnId, TurnScope, TurnWorkspace,
    },
    task::{
        CheckpointEntry, CheckpointFile, CheckpointObject, CheckpointObjectStatus, CheckpointPage,
        CheckpointPin, CheckpointPlaintext, CheckpointTransfer,
    },
};
use base64::{Engine as _, prelude::BASE64_STANDARD};
use std::{collections::BTreeMap, os::unix::fs::MetadataExt as _, path::Path, time::Duration};
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

const ID: &str = "019f0000-0000-7000-8000-000000000001";
const PAGE: &str = "019f0000-0000-7000-8000-000000000002";

#[tokio::test]
async fn ordinary_chunks_from_separate_files_share_a_live_batch_in_manifest_order()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let Encrypted {
        mut objects,
        payloads,
    } = encrypted()?;
    let server = Server::simultaneous(payloads).await?;
    for object in &mut objects {
        let mut url = url::Url::parse(&server.url)?;
        url.set_path(&format!("/{}", object.object_id));
        object.transfer = Some(CheckpointTransfer {
            url: url.to_string(),
            headers: BTreeMap::new(),
            expires_at: clock::after(clock::now(), Duration::from_secs(30)),
        });
    }
    let [first, second, third, fourth] = objects.as_slice() else {
        return Err(error::config("ordinary fixture requires four objects").into());
    };
    let page = page(vec![
        CheckpointEntry::Directory {
            path: "dir".to_owned(),
        },
        CheckpointEntry::File(CheckpointFile {
            path: "dir/script".to_owned(),
            executable: true,
            size: 14,
            objects: vec![fourth.object_id.clone(), second.object_id.clone()],
        }),
        CheckpointEntry::File(CheckpointFile {
            path: "dir/text".to_owned(),
            executable: false,
            size: 14,
            objects: vec![third.object_id.clone(), first.object_id.clone()],
        }),
        CheckpointEntry::Symlink {
            path: "link".to_owned(),
            target: "dir/script".to_owned(),
        },
    ]);
    let root = tempfile::tempdir()?;
    restore(root.path(), page, objects, Some(server.transfers()), None).await?;
    assert_files(root.path())?;
    server.finish().await?;
    Ok(())
}

fn assert_files(root: &Path) -> Result<()> {
    assert_eq!(std::fs::read(root.join("dir/script"))?, b"part-3\npart-1\n");
    assert_eq!(std::fs::read(root.join("dir/text"))?, b"part-2\npart-0\n");
    assert_eq!(
        std::fs::read_link(root.join("link"))?,
        Path::new("dir/script")
    );
    let metadata = std::fs::metadata(root.join("dir/script"))?;
    assert_eq!(metadata.mode() & 0o777, 0o700);
    assert_eq!(metadata.uid(), rustix::process::getuid().as_raw());
    assert_eq!(metadata.gid(), rustix::process::getgid().as_raw());
    Ok(())
}

#[tokio::test]
async fn cancellation_after_the_manifest_prevents_directory_creation() -> Result<()> {
    let root = tempfile::tempdir()?;
    let page = page(vec![CheckpointEntry::Directory {
        path: "dir".to_owned(),
    }]);
    assert!(
        restore(
            root.path(),
            page,
            Vec::new(),
            None,
            Some(CancellationToken::new())
        )
        .await
        .is_err_and(|failure| failure.code() == af_core::error_code::INTERNAL_OPERATION_FAILED)
    );
    assert!(!root.path().join("dir").exists());
    Ok(())
}

fn page(entries: Vec<CheckpointEntry>) -> CheckpointPage {
    CheckpointPage {
        page_number: 0,
        entries,
        history_objects: Vec::new(),
    }
}

fn scope() -> TurnScope {
    TurnScope {
        workspace_id: TurnId(ID.to_owned()),
        fleet_id: TurnId(ID.to_owned()),
        event_id: TurnId(ID.to_owned()),
        setup_revision_id: TurnId(ID.to_owned()),
        workspace: TurnWorkspace::Files {
            toolbox_digest: "a".repeat(64),
        },
    }
}

async fn restore(
    root: &Path,
    page: CheckpointPage,
    objects: Vec<CheckpointObject>,
    transfers: Option<Transfers>,
    cancel_after_read: Option<CancellationToken>,
) -> Result<()> {
    let digest = af_core::etag::sha256_hex(&serde_json::to_vec(&page).map_err(error::encode)?);
    let manifest = af_crypto::checkpoint_digest::manifest_root([(PAGE, digest.as_str())]);
    let stop = cancel_after_read.clone().unwrap_or_default();
    let (plane, _calls) = fixture_plane(page, objects, cancel_after_read);

    let run = Uuid7::parse(ID)?;
    let default_transfers = Transfers::new()?;
    let native = NativeTurn::new(&plane, &run, 1, &default_transfers);
    let scope = scope();
    let reference = TurnCheckpointRef {
        checkpoint_id: TurnId(ID.to_owned()),
        pin_id: TurnId(PAGE.to_owned()),
    };
    let mut session = TurnSession::new(&native, &scope, reference, pin(), &stop)?;
    if let Some(transfers) = transfers {
        session.transfers = transfers;
    }
    session
        .restore_files(
            &[TurnId(PAGE.to_owned())],
            &manifest,
            (
                root.to_owned(),
                (
                    rustix::process::getuid().as_raw(),
                    rustix::process::getgid().as_raw(),
                ),
            ),
            None,
        )
        .await
}

fn fixture_plane(
    page: CheckpointPage,
    objects: Vec<CheckpointObject>,
    cancel_after_read: Option<CancellationToken>,
) -> (crate::client::ControlPlane, test_support::Calls) {
    test_support::plane(move |call| {
        let result = (|| -> Result<Response> {
            let body = call
                .body
                .as_ref()
                .ok_or_else(|| error::config("native body missing"))?;
            let request: TurnCheckpointCommand =
                serde_json::from_slice(body).map_err(error::malformed(call.verb))?;
            match request.action {
                Action::PageRead { .. } => {
                    if let Some(stop) = &cancel_after_read {
                        stop.cancel();
                    }
                    Ok(Response::PageRead(page.clone()))
                }
                Action::Authorize { .. } => Ok(Response::Authorized(objects.clone())),
                _ => Err(error::config("unexpected ordinary file request")),
            }
        })();
        match result {
            Ok(response) => test_support::json(&response),
            Err(failure) => Answer::Fail(failure),
        }
    })
}

fn pin() -> CheckpointPin {
    let deadline = clock::after(clock::now(), Duration::from_mins(15));
    CheckpointPin {
        id: PAGE.to_owned(),
        expires_at: deadline,
        operation_deadline: deadline,
        resource_version: 1,
    }
}

struct Encrypted {
    objects: Vec<CheckpointObject>,
    payloads: BTreeMap<String, Vec<u8>>,
}

fn encrypted() -> Result<Encrypted> {
    let workspace = Uuid7::parse(ID)?;
    let mut objects = Vec::new();
    let mut payloads = BTreeMap::new();
    for index in 0..4 {
        let plaintext = format!("part-{index}\n").into_bytes();
        let mut object = CheckpointObject {
            object_id: format!("019f0000-0000-7000-8000-{:012}", index + 10),
            repository: format!("turn:{ID}"),
            status: CheckpointObjectStatus::Reused,
            plaintext: CheckpointPlaintext {
                sha256: af_core::etag::sha256_hex(&plaintext),
                size: 7,
                kind: "file".to_owned(),
            },
            nonce: Some(BASE64_STANDARD.encode([7_u8; af_crypto::NONCE_LEN])),
            data_key: Some(BASE64_STANDARD.encode([17_u8; af_crypto::KEY_LEN])),
            ciphertext_sha256: None,
            ciphertext_length: None,
            transfer: None,
        };
        let bytes = transfer::seal(&workspace, &mut object.clone(), Zeroizing::new(plaintext))?;
        object.ciphertext_sha256 = Some(af_core::etag::sha256_hex(&bytes));
        object.ciphertext_length = Some(crate::byte_count(bytes.len()));
        payloads.insert(format!("/{}", object.object_id), bytes);
        objects.push(object);
    }
    Ok(Encrypted { objects, payloads })
}

#[path = "upload_tests.rs"]
mod upload_tests;

#[path = "capture_tests.rs"]
mod capture_tests;

#[path = "session_tests.rs"]
mod session_tests;
