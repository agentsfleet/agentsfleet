//! An ordinary capture seals and sends a bounded batch through the native owner,
//! and counts only the PUTs the bucket accepted.

use super::session_tests::reference;
use super::*;
use crate::recovery::{
    Object,
    uploads::{Publisher as _, UploadBatch},
};

type Checked<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[tokio::test]
async fn ordinary_uploads_share_one_authorization_and_report_what_was_accepted() -> Checked {
    let Encrypted { objects, payloads } = encrypted()?;
    let mut server = Server::simultaneous(payloads.clone()).await?;
    let stop = CancellationToken::new();
    let Staged {
        pending,
        authorized,
    } = stage(objects, &server.url)?;
    let expected: Vec<_> = authorized
        .iter()
        .map(|object| object.object_id.clone())
        .collect();
    let (plane, mut calls) = upload_plane(authorized);
    let run = Uuid7::parse(ID)?;
    let transfers = server.transfers();
    let native = NativeTurn::new(&plane, &run, 1, &transfers);
    let reference = TurnCheckpointRef {
        checkpoint_id: CheckpointOperationId(ID.to_owned()),
        pin_id: CheckpointOperationId(PAGE.to_owned()),
    };
    let mut session = TurnSession::new(&native, &scope(), reference, pin(), &stop)?;
    let mut accepted = session.upload_batch(pending).await?;
    accepted.sort();
    assert_eq!(accepted, expected, "every accepted PUT is reported");
    assert_eq!(
        test_support::drain(&mut calls).len(),
        1,
        "no object is read back"
    );
    for _ in 0..4 {
        let seen = server.seen.try_recv()?;
        let seen_path = seen.path.split('?').next().ok_or("missing request path")?;
        assert_eq!(seen.method, http::Method::PUT);
        assert_eq!(
            seen.bytes.as_ref(),
            payloads.get(seen_path).ok_or("unknown PUT")?
        );
    }
    server.finish().await
}

struct Staged {
    pending: Vec<(Object, Zeroizing<Vec<u8>>)>,
    authorized: Vec<CheckpointObject>,
}

/// Each fixture object paired with the plaintext it registered, its upload
/// link under `url`.
fn stage(objects: Vec<CheckpointObject>, url: &str) -> Checked<Staged> {
    let contents: Vec<_> = (0..4)
        .map(|index| format!("part-{index}\n").into_bytes())
        .collect();
    let mut pending = Vec::new();
    let mut authorized = Vec::new();
    for mut object in objects {
        let bytes = contents
            .iter()
            .find(|bytes| af_core::etag::sha256_hex(bytes) == object.plaintext.sha256)
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
        pending.push((Object::from(object), Zeroizing::new(bytes.clone())));
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
                Action::Uploads(TurnCheckpointUploads {
                    objects: descriptors,
                    ..
                }) => {
                    assert_eq!(descriptors.len(), objects.len());
                    Ok(Response::Uploads(objects.clone()))
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
    /// The batch's PUT authority lacks its create-only precondition.
    NoCreateOnly,
}

/// An owner that hands out `objects` for upload, each link as `fault` says.
fn uploads_plane(
    objects: Vec<CheckpointObject>,
    fault: Option<Owner>,
) -> (crate::client::ControlPlane, test_support::Calls) {
    session_tests::scripted(move |action| match action {
        Action::Uploads(_) => Ok(Response::Uploads(
            objects
                .iter()
                .cloned()
                .map(|mut object| {
                    if let (Some(Owner::NoCreateOnly), Some(transfer)) =
                        (fault, object.transfer.as_mut())
                    {
                        transfer.headers.clear();
                    }
                    object
                })
                .collect(),
        )),
        _ => Err(error::config("unexpected upload request")),
    })
}

#[tokio::test]
async fn a_put_refused_before_sending_fails_the_batch_and_reads_nothing_back() -> Checked {
    let Encrypted { objects, .. } = encrypted()?;
    let stop = CancellationToken::new();
    let Staged {
        pending,
        authorized,
    } = stage(objects, "https://objects.invalid")?;
    let (plane, mut calls) = uploads_plane(authorized, Some(Owner::NoCreateOnly));
    let run = Uuid7::parse(ID)?;
    let transfers = Transfers::new()?;
    let native = NativeTurn::new(&plane, &run, 1, &transfers);
    let mut session = TurnSession::new(&native, &scope(), reference(), pin(), &stop)?;

    let uploaded = session.upload_batch(pending).await;

    assert_eq!(
        test_support::detail(&uploaded),
        Some("checkpoint upload lacks create-only authority")
    );
    assert!(matches!(
        session_tests::sent(&mut calls)?.as_slice(),
        [Action::Uploads(_)]
    ));
    Ok(())
}

fn copy(object: &Object) -> Object {
    Object::from(CheckpointObject::clone(object))
}

#[test]
fn a_physical_batch_must_be_nonempty_owned_and_distinct() -> Checked {
    let Encrypted { objects, .. } = encrypted()?;
    let workspace = Uuid7::parse(ID)?;

    let empty = UploadBatch::seal(&workspace, Vec::new());
    assert_eq!(
        test_support::detail(&empty),
        Some("checkpoint physical batch exceeds its bound")
    );

    let Staged { mut pending, .. } = stage(objects.clone(), "https://objects.invalid")?;
    if let Some((object, _chunk)) = pending.first_mut() {
        object.status = CheckpointObjectStatus::Reused;
    }
    let unowned = UploadBatch::seal(&workspace, pending);
    assert_eq!(
        test_support::detail(&unowned),
        Some("checkpoint physical batch has no upload owner")
    );

    let Staged { pending, .. } = stage(objects, "https://objects.invalid")?;
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
    let workspace = Uuid7::parse(ID)?;
    let Staged {
        pending,
        authorized,
    } = stage(objects, "https://objects.invalid")?;
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
