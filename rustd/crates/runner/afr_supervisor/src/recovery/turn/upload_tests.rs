//! An ordinary capture sends and verifies a bounded batch through the native owner.

use super::session_tests::reference;
use super::*;
use crate::recovery::{
    Object,
    catalog::{Catalog, Chunk},
    files::Workspace,
    uploads::UploadBatch,
};
use af_wire::task::CheckpointTransferPurpose;
use std::sync::Arc;

type Checked<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[tokio::test]
async fn ordinary_uploads_share_one_authorization_and_one_readback_request() -> Checked {
    let Encrypted { objects, payloads } = encrypted()?;
    let mut server = Server::simultaneous(payloads.clone()).await?;
    let root = tempfile::tempdir()?;
    let stop = CancellationToken::new();
    let Staged {
        pending,
        authorized,
    } = stage(root.path(), objects, &server.url, &stop)?;
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

struct Staged {
    pending: Vec<(Object, Chunk)>,
    authorized: Vec<CheckpointObject>,
}

fn stage(
    root: &Path,
    objects: Vec<CheckpointObject>,
    url: &str,
    stop: &CancellationToken,
) -> Checked<Staged> {
    for index in 0..4 {
        std::fs::write(
            root.join(format!("file-{index}")),
            format!("part-{index}\n"),
        )?;
    }
    let root = Arc::new(Workspace::open(root)?);
    let catalog = Catalog::build(Some(&root), Zeroizing::new(Vec::new()), Vec::new(), stop)?;
    let mut pending = Vec::new();
    let mut authorized = Vec::new();
    for mut object in objects {
        let chunk = catalog
            .chunks
            .values()
            .find(|chunk| chunk.descriptor.sha256 == object.plaintext.sha256)
            .ok_or("missing fixture chunk")?;
        object.status = CheckpointObjectStatus::Missing;
        let mut target = url::Url::parse(url)?;
        target.set_path(&format!("/{}", object.object_id));
        object.transfer = Some(CheckpointTransfer {
            url: target.to_string(),
            headers: BTreeMap::from([("if-none-match".to_owned(), "*".to_owned())]),
            expires_at: clock::after(clock::now(), Duration::from_secs(30)),
        });
        authorized.push(object.clone());
        pending.push((Object::from(object), chunk.clone()));
    }
    Ok(Staged {
        pending,
        authorized,
    })
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

/// How the native owner misbehaves for one upload batch.
#[derive(Clone, Copy)]
enum Owner {
    /// The upload request is answered with another command's response.
    WrongReply,
    /// The batch's PUT authority lacks its create-only precondition.
    NoCreateOnly,
}

/// An owner that hands out `objects` for upload and then reports every one
/// of them as still missing, so no PUT is ever verified.
fn unverifying_plane(
    objects: Vec<CheckpointObject>,
    fault: Option<Owner>,
) -> (crate::client::ControlPlane, test_support::Calls) {
    super::session_tests::scripted(move |action| match action {
        Action::Uploads { .. } => match fault {
            Some(Owner::WrongReply) => Ok(Response::PinReleased),
            Some(Owner::NoCreateOnly) => Ok(Response::Uploads(
                objects
                    .iter()
                    .cloned()
                    .map(|mut object| {
                        if let Some(transfer) = object.transfer.as_mut() {
                            transfer.headers.clear();
                        }
                        object
                    })
                    .collect(),
            )),
            None => Ok(Response::Uploads(objects.clone())),
        },
        Action::Authorize {
            purpose: CheckpointTransferPurpose::Upload,
            ..
        } => Ok(Response::Authorized(
            objects
                .iter()
                .cloned()
                .map(|mut object| {
                    object.transfer = None;
                    object
                })
                .collect(),
        )),
        _ => Err(error::config("unexpected upload request")),
    })
}

#[tokio::test]
async fn an_upload_answered_with_another_commands_response_puts_nothing() -> Checked {
    let Encrypted { objects, .. } = encrypted()?;
    let root = tempfile::tempdir()?;
    let stop = CancellationToken::new();
    let Staged {
        pending,
        authorized,
    } = stage(root.path(), objects, "https://objects.invalid", &stop)?;
    let (plane, mut calls) = unverifying_plane(authorized, Some(Owner::WrongReply));
    let lease = Uuid7::parse(ID)?;
    let transfers = Transfers::new()?;
    let native = NativeTurn::new(&plane, &lease, 1, &transfers);
    let mut session = TurnSession::new(&native, &scope(), reference(), pin(), &stop)?;

    let uploaded = session.upload_batch(pending).await;

    assert_eq!(
        test_support::detail(&uploaded),
        Some("turn upload returned another command's response")
    );
    assert!(matches!(
        super::session_tests::sent(&mut calls)?.as_slice(),
        [Action::Uploads { objects, .. }] if objects.len() == 4
    ));
    Ok(())
}

#[tokio::test]
async fn stored_chunks_the_owner_does_not_verify_fail_the_upload() -> Checked {
    let Encrypted { objects, payloads } = encrypted()?;
    let mut server = Server::simultaneous(payloads).await?;
    let root = tempfile::tempdir()?;
    let stop = CancellationToken::new();
    let Staged {
        pending,
        authorized,
    } = stage(root.path(), objects, &server.url, &stop)?;
    let (plane, mut calls) = unverifying_plane(authorized, None);
    let lease = Uuid7::parse(ID)?;
    let transfers = server.transfers();
    let native = NativeTurn::new(&plane, &lease, 1, &transfers);
    let mut session = TurnSession::new(&native, &scope(), reference(), pin(), &stop)?;

    let uploaded = session.upload_batch(pending).await;

    assert_eq!(
        test_support::detail(&uploaded),
        Some("turn upload was not authenticated by its durable owner")
    );
    assert!(matches!(
        super::session_tests::sent(&mut calls)?.as_slice(),
        [Action::Uploads { .. }, Action::Authorize { .. }]
    ));
    for _ in 0..4 {
        assert_eq!(server.seen.try_recv()?.method, axum::http::Method::PUT);
    }
    server.finish().await
}

#[tokio::test]
async fn a_put_refused_before_sending_is_the_failure_an_unverified_batch_reports() -> Checked {
    let Encrypted { objects, .. } = encrypted()?;
    let root = tempfile::tempdir()?;
    let stop = CancellationToken::new();
    let Staged {
        pending,
        authorized,
    } = stage(root.path(), objects, "https://objects.invalid", &stop)?;
    let (plane, mut calls) = unverifying_plane(authorized, Some(Owner::NoCreateOnly));
    let lease = Uuid7::parse(ID)?;
    let transfers = Transfers::new()?;
    let native = NativeTurn::new(&plane, &lease, 1, &transfers);
    let mut session = TurnSession::new(&native, &scope(), reference(), pin(), &stop)?;

    let uploaded = session.upload_batch(pending).await;

    assert_eq!(
        test_support::detail(&uploaded),
        Some("checkpoint upload lacks create-only authority")
    );
    assert!(matches!(
        super::session_tests::sent(&mut calls)?.as_slice(),
        [Action::Uploads { .. }, Action::Authorize { .. }]
    ));
    Ok(())
}

fn copy(object: &Object) -> Object {
    Object::from(CheckpointObject::clone(object))
}

#[test]
fn a_physical_batch_must_be_nonempty_owned_and_distinct() -> Checked {
    let Encrypted { objects, .. } = encrypted()?;
    let root = tempfile::tempdir()?;
    let stop = CancellationToken::new();
    let workspace = Uuid7::parse(ID)?;

    let empty = UploadBatch::seal(&workspace, Vec::new());
    assert_eq!(
        test_support::detail(&empty),
        Some("checkpoint physical batch exceeds its bound")
    );

    let Staged { mut pending, .. } = stage(
        root.path(),
        objects.clone(),
        "https://objects.invalid",
        &stop,
    )?;
    if let Some((object, _chunk)) = pending.first_mut() {
        object.status = CheckpointObjectStatus::Reused;
    }
    let unowned = UploadBatch::seal(&workspace, pending);
    assert_eq!(
        test_support::detail(&unowned),
        Some("checkpoint physical batch has no upload owner")
    );

    let Staged { pending, .. } = stage(root.path(), objects, "https://objects.invalid", &stop)?;
    let (object, chunk) = pending.first().ok_or("missing staged chunk")?;
    let repeated = UploadBatch::seal(
        &workspace,
        vec![(copy(object), chunk.clone()), (copy(object), chunk.clone())],
    );
    assert_eq!(
        test_support::detail(&repeated),
        Some("checkpoint upload identity repeated")
    );
    Ok(())
}

#[tokio::test]
async fn an_upload_authority_naming_one_object_twice_sends_nothing() -> Checked {
    let Encrypted { objects, .. } = encrypted()?;
    let root = tempfile::tempdir()?;
    let stop = CancellationToken::new();
    let workspace = Uuid7::parse(ID)?;
    let Staged {
        pending,
        authorized,
    } = stage(root.path(), objects, "https://objects.invalid", &stop)?;
    let pair: Vec<_> = pending.into_iter().take(2).collect();
    let mut batch = UploadBatch::seal(&workspace, pair)?;
    let first = authorized.first().ok_or("missing authorized chunk")?;

    let sent = batch
        .put(
            vec![first.clone(), first.clone()],
            &format!("turn:{ID}"),
            &Transfers::new()?,
        )
        .await;

    assert_eq!(
        test_support::detail(&sent),
        Some("checkpoint upload identity repeated")
    );
    Ok(())
}
