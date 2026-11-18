//! An ordinary capture sends and verifies a bounded batch through the native owner.

use super::*;
use crate::recovery::{
    Object,
    catalog::{Catalog, Chunk},
    files::Workspace,
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
    } = stage(root.path(), objects, &server, &stop)?;
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
    server: &Server,
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
