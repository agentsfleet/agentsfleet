//! An ordinary file restore reads authenticated chunks in manifest order and
//! lands them through the sandbox's engine.

use super::{NativeTurn, reading::TurnReading, session::TurnSession};
use crate::recovery::source::land;
use crate::recovery::{transfer::Transfers, transfer::http_tests::server::Server};
use crate::{
    Result, error,
    test_support::{self, Answer, Lander},
};
use af_core::{clock, id::Uuid7};
use af_wire::{
    durable_task::{
        CheckpointEntry, CheckpointFile, CheckpointObject, CheckpointObjectStatus, CheckpointPage,
        CheckpointPin, CheckpointPlaintext, CheckpointTransfer,
    },
    run::{
        CheckpointOperationId, TurnCheckpointAction as Action, TurnCheckpointCommand,
        TurnCheckpointCommit, TurnCheckpointObjects, TurnCheckpointPageRead,
        TurnCheckpointPageWrite, TurnCheckpointPinRenew, TurnCheckpointRef,
        TurnCheckpointResponse as Response, TurnCheckpointUploads, TurnScope, TurnWorkspace,
    },
};
use base64::{Engine as _, prelude::BASE64_STANDARD};
use std::{collections::BTreeMap, os::unix::fs::MetadataExt as _, path::Path, time::Duration};
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

const ID: &str = "019f0000-0000-7000-8000-000000000001";
const PAGE: &str = "019f0000-0000-7000-8000-000000000002";

#[tokio::test]
async fn ordinary_chunks_from_separate_files_land_in_manifest_order()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let Encrypted {
        mut objects,
        payloads,
    } = encrypted()?;
    let server = Server::serving(payloads).await?;
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
    let lander = Lander::on(&"a".repeat(64)).await;
    restore(&lander, page, objects, Some(server.transfers()), None).await?;
    assert_files(lander.workspace())?;
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
    let script = std::fs::metadata(root.join("dir/script"))?.mode();
    let text = std::fs::metadata(root.join("dir/text"))?.mode();
    assert_eq!((script & 0o111, text & 0o111), (0o111, 0));
    Ok(())
}

#[tokio::test]
async fn cancellation_after_the_manifest_prevents_directory_creation() -> Result<()> {
    let lander = Lander::on(&"a".repeat(64)).await;
    let page = page(vec![CheckpointEntry::Directory {
        path: "dir".to_owned(),
    }]);
    assert!(
        restore(
            &lander,
            page,
            Vec::new(),
            None,
            Some(CancellationToken::new())
        )
        .await
        .is_err_and(|failure| failure.code() == af_core::error_code::INTERNAL_OPERATION_FAILED)
    );
    assert!(!lander.workspace().join("dir").exists());
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
        workspace_id: CheckpointOperationId(ID.to_owned()),
        fleet_id: CheckpointOperationId(ID.to_owned()),
        turn_id: CheckpointOperationId(ID.to_owned()),
        workspace: TurnWorkspace::Files {
            toolbox_digest: "a".repeat(64),
            sandbox: test_support::recorded(),
        },
    }
}

async fn restore(
    lander: &Lander,
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
        checkpoint_id: CheckpointOperationId(ID.to_owned()),
        pin_id: CheckpointOperationId(PAGE.to_owned()),
    };
    let mut session = TurnSession::new(&native, &scope, reference, pin(), &stop)?;
    if let Some(transfers) = transfers {
        session.transfers = transfers;
    }
    let pages = [CheckpointOperationId(PAGE.to_owned())];
    let reading = TurnReading {
        session: &mut session,
        chain: &[],
        pages: &pages,
        digest: &manifest,
    };
    land(
        reading,
        (run.clone(), 1),
        lander.environment(),
        lander.landing(),
    )
    .await
    .map(drop)
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
                Action::PageRead(_) => {
                    if let Some(stop) = &cancel_after_read {
                        stop.cancel();
                    }
                    Ok(Response::PageRead(page.clone()))
                }
                Action::Authorize(asked) => Ok(Response::Authorized(
                    objects
                        .iter()
                        .filter(|object| asked.object_ids.iter().any(|id| id.0 == object.object_id))
                        .cloned()
                        .collect(),
                )),
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
            cipher: af_wire::durable_task::CheckpointCipher::default(),
            nonce: Some(BASE64_STANDARD.encode([7_u8; af_crypto::NONCE_LEN])),
            data_key: Some(BASE64_STANDARD.encode([17_u8; af_crypto::KEY_LEN])),
            ciphertext_sha256: None,
            ciphertext_length: None,
            transfer: None,
        };
        let bytes =
            af_chunk_store::seal(&workspace, &mut object.clone(), Zeroizing::new(plaintext))?;
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

#[path = "pause_tests.rs"]
mod pause_tests;

#[path = "session_tests.rs"]
mod session_tests;
