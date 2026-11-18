//! Ordinary file restore authenticates concurrent chunks and preserves their manifest order.

use super::{NativeTurn, session::TurnSession};
use crate::recovery::{
    Object, catalog::Catalog, files::Workspace, transfer, transfer::Transfers,
    transfer::http_tests::server::Server,
};
use crate::{
    Result, error,
    test_support::{self, Answer},
};
use af_core::{clock, id::Uuid7};
use af_wire::{
    lease::{
        TurnCheckpointAction as Action, TurnCheckpointCommand, TurnCheckpointRef,
        TurnCheckpointResponse as Response, TurnId, TurnScope, TurnWorkspace,
    },
    task::{
        CheckpointEntry, CheckpointFile, CheckpointObject, CheckpointObjectStatus, CheckpointPage,
        CheckpointPin, CheckpointPlaintext, CheckpointTransfer, CheckpointTransferPurpose,
    },
};
use base64::{Engine as _, prelude::BASE64_STANDARD};
use std::{
    collections::BTreeMap, os::unix::fs::MetadataExt as _, path::Path, sync::Arc, time::Duration,
};
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

const ID: &str = "019f0000-0000-7000-8000-000000000001";
const PAGE: &str = "019f0000-0000-7000-8000-000000000002";

#[tokio::test]
async fn ordinary_chunks_from_separate_files_share_a_live_batch_in_manifest_order()
-> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
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
    let ordered: Vec<_> = [3, 1, 2, 0]
        .into_iter()
        .map(|index| {
            objects
                .get(index)
                .map(|object| object.object_id.clone())
                .ok_or_else(|| error::config("ordinary fixture object missing"))
        })
        .collect::<Result<_>>()?;
    let page = page(vec![
        CheckpointEntry::Directory {
            path: "dir".to_owned(),
        },
        CheckpointEntry::File(CheckpointFile {
            path: "dir/script".to_owned(),
            executable: true,
            size: 14,
            objects: ordered.iter().take(2).cloned().collect(),
        }),
        CheckpointEntry::File(CheckpointFile {
            path: "dir/text".to_owned(),
            executable: false,
            size: 14,
            objects: ordered.into_iter().skip(2).collect(),
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
async fn ordinary_uploads_share_one_authorization_and_one_readback_request()
-> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let Encrypted { objects, payloads } = encrypted()?;
    let mut server = Server::simultaneous(payloads.clone()).await?;
    let root = tempfile::tempdir()?;
    for index in 0..4 {
        std::fs::write(
            root.path().join(format!("file-{index}")),
            format!("part-{index}\n"),
        )?;
    }
    let stop = CancellationToken::new();
    let root = Arc::new(Workspace::open(root.path())?);
    let catalog = Catalog::build(Some(&root), Zeroizing::new(Vec::new()), Vec::new(), &stop)?;
    let mut pending = Vec::new();
    let mut authorized = Vec::new();
    for mut object in objects {
        let chunk = catalog
            .chunks
            .values()
            .find(|chunk| chunk.descriptor.sha256 == object.plaintext.sha256)
            .ok_or("missing fixture chunk")?;
        object.status = CheckpointObjectStatus::Missing;
        let mut url = url::Url::parse(&server.url)?;
        url.set_path(&format!("/{}", object.object_id));
        object.transfer = Some(CheckpointTransfer {
            url: url.to_string(),
            headers: BTreeMap::from([("if-none-match".to_owned(), "*".to_owned())]),
            expires_at: clock::after(clock::now(), Duration::from_secs(30)),
        });
        authorized.push(object.clone());
        pending.push((Object::from(object), chunk.clone()));
    }
    let (plane, mut calls) = upload_plane(authorized);
    let lease = Uuid7::parse(ID)?;
    let transfers = server.transfers();
    let native = NativeTurn::new(&plane, &lease, 1, &transfers);
    let reference = TurnCheckpointRef {
        checkpoint_id: TurnId(ID.to_owned()),
        pin_id: TurnId(PAGE.to_owned()),
    };
    let mut session = TurnSession::new(&native, &scope(), reference, pin(), &stop)?;
    session.upload_batch(pending).await?;
    assert_eq!(test_support::drain(&mut calls).len(), 2);
    for _ in 0..4 {
        let seen = server.seen.try_recv()?;
        let seen_path = seen.path.split('?').next().ok_or("missing request path")?;
        assert_eq!(seen.method, axum::http::Method::PUT);
        assert_eq!(
            seen.bytes.as_ref(),
            payloads.get(seen_path).ok_or("unknown PUT")?
        );
    }
    server.finish().await
}

fn upload_plane(
    objects: Vec<CheckpointObject>,
) -> (crate::client::ControlPlane, test_support::Calls) {
    test_support::plane(move |call| {
        let result = (|| -> Result<Response> {
            let body = call
                .body
                .as_ref()
                .ok_or_else(|| error::config("missing command"))?;
            let request: TurnCheckpointCommand =
                serde_json::from_slice(body).map_err(error::encode)?;
            match request.action {
                Action::Uploads {
                    objects: descriptors,
                    ..
                } => {
                    assert_eq!(descriptors.len(), objects.len());
                    Ok(Response::Uploads(objects.clone()))
                }
                Action::Authorize {
                    purpose: CheckpointTransferPurpose::Upload,
                    object_ids,
                    ..
                } => {
                    assert_eq!(object_ids.len(), objects.len());
                    let verified = objects
                        .iter()
                        .cloned()
                        .map(|mut object| {
                            object.status = CheckpointObjectStatus::Reused;
                            object.transfer = None;
                            object
                        })
                        .collect();
                    Ok(Response::Authorized(verified))
                }
                _ => Err(error::config("unexpected upload request")),
            }
        })();
        match result {
            Ok(response) => test_support::json(&response),
            Err(failure) => Answer::Fail(failure),
        }
    })
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

    let lease = Uuid7::parse(ID)?;
    let default_transfers = Transfers::new()?;
    let native = NativeTurn::new(&plane, &lease, 1, &default_transfers);
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
