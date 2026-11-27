//! A publication reads nothing back: the commit records the uploads the runner
//! saw accepted, and refuses a generation naming any other.

use super::{
    Checked,
    key_fixture::{begun, page_of, registered, save},
    storage::Files,
};
use af_core::{error_code, id::Uuid7};
use af_wire::{
    durable_task::{CheckpointObject, CheckpointPage},
    run::{
        CheckpointOperationId, TurnCheckpointAbandon, TurnCheckpointAction as Action,
        TurnCheckpointCommit, TurnCheckpointPageWrite, TurnCheckpointRef,
        TurnCheckpointResponse as Response, TurnResume,
    },
};

/// Where every checkpoint object is stored, beside the bucket's own probes.
const OBJECTS: &str = "/checkpoint-test/workspaces/";

// Value: protects=Dimension 2.7, a publication stores each new chunk once and
// reads none back, and an object whose PUT the runner reports failed never
// becomes verified or part of a head; fails_when=the daemon GETs or HEADs an
// object, the commit verifies an object the runner did not report, or it
// publishes a generation naming one; why_new=the read-back is gone and its
// replacement had no case; seam=the production turn verbs over live Postgres
// and real SDK requests to the private HTTPS fixture.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_publication_reads_no_object_back() -> Checked {
    let files = Files::new().await?;
    let since = files
        .server
        .requests
        .lock()
        .map_err(|_poisoned| "log")?
        .len();
    let contents: Vec<Vec<u8>> = (0..10).map(|n| format!("chunk {n}").into_bytes()).collect();
    let contents: Vec<&[u8]> = contents.iter().map(Vec::as_slice).collect();
    let (saved, objects) = save(&files, &contents, None).await?;
    let seen = files
        .server
        .requests
        .lock()
        .map_err(|_poisoned| "log")?
        .get(since..)
        .ok_or("the requests since activation")?
        .to_vec();
    let count = |method: &str| {
        seen.iter()
            .filter(|(seen, path)| seen == method && path.starts_with(OBJECTS))
            .count()
    };
    assert_eq!(count("PUT"), 10, "each new chunk is stored once");
    assert_eq!(
        (count("GET"), count("HEAD")),
        (0, 0),
        "nothing is read back"
    );
    assert_eq!(seen.len(), 10, "the bucket saw nothing but the uploads");
    for object in &objects {
        assert!(verified(&files, object).await?, "an accepted upload counts");
    }
    let (refused, unreported) = unreported(&files, &saved).await?;
    assert_eq!(refused, Some(error_code::RUN_CHECKPOINT_INCOMPATIBLE));
    assert!(!verified(&files, &unreported).await?);
    let head = files.case.counts().await?.3;
    assert_eq!(head, Some(Uuid7::parse(&saved.checkpoint_id.0)?));
    files.finish().await;
    Ok(())
}

// Value: protects=content a failed publication uploaded can be saved again
// once its writer can no longer upload, as a new object, while a live writer's
// stays its own; fails_when=registration keeps answering the abandoned object
// busy, reuses it unverified, or forgets to purge it; why_new=registration's
// hand-back of an unverified object had no case; seam=the production turn
// verbs over live Postgres and the private HTTPS fixture.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_failed_publication_gives_its_content_back_once_its_writer_lapses() -> Checked {
    let files = Files::new().await?;
    let (saved, _) = save(&files, &[b"base state"], None).await?;
    let (_, abandoned) = unreported(&files, &saved).await?;
    expire_writers(&files).await?;
    let (_, again) = save(&files, &[b"kept state", b"lost state"], Some(&saved)).await?;
    let fresh = again.last().ok_or("the lost content again")?;
    assert_ne!(fresh.object_id, abandoned.object_id, "a new object");
    assert!(verified(&files, fresh).await?);
    let mut connection = files.case.fixtures.database.acquire().await?;
    let (rows, purges): (i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM runner.checkpoint_objects WHERE id=$1),
                (SELECT count(*) FROM runner.checkpoint_purges WHERE checkpoint_object_id=$1)",
    )
    .bind(Uuid7::parse(&abandoned.object_id)?)
    .fetch_one(&mut *connection)
    .await?;
    drop(connection);
    assert_eq!((rows, purges), (0, 1), "the abandoned object is purged");
    files.finish().await;
    Ok(())
}

/// A save over `head` of two chunks, both PUT, whose commit reports only the
/// first accepted, then abandoned as a runner abandons a failed publication:
/// the commit's refusal code, and the unreported object.
async fn unreported(
    files: &Files,
    head: &TurnResume,
) -> Checked<(Option<error_code::ErrorCode>, CheckpointObject)> {
    let contents: [&[u8]; 2] = [b"kept state", b"lost state"];
    let (plan, history) = begun(files, &contents, Some(head)).await?;
    let reference = TurnCheckpointRef {
        checkpoint_id: plan.checkpoint_id.clone(),
        pin_id: CheckpointOperationId(plan.pin.id.clone()),
    };
    let (objects, uploaded) = registered(files, &reference, &contents).await?;
    assert_eq!(uploaded.len(), 2, "both chunks were PUT");
    let page_id = plan.page_ids.first().ok_or("page")?.clone();
    let digest = written(files, &reference, &page_id, page_of(&contents, &objects)?).await?;
    let commit = super::valid(super::publication::command(
        &files.case,
        Action::Commit(TurnCheckpointCommit {
            checkpoint: reference.clone(),
            root: history,
            manifest_root: af_crypto::checkpoint_digest::manifest_root([(
                page_id.0.as_str(),
                digest.as_str(),
            )]),
            folds: Vec::new(),
            uploaded: uploaded.into_iter().take(1).collect(),
        }),
    ))?;
    let refused = files
        .store
        .execute(&files.case.runs, files.case.run(), &commit)
        .await
        .err()
        .map(|failure| failure.code());
    files
        .execute(Action::Abandon(TurnCheckpointAbandon {
            checkpoint: reference,
        }))
        .await?;
    let lost = objects.into_iter().last().ok_or("the unreported object")?;
    Ok((refused, lost))
}

/// Writes `page` as the plan's page `page_id`, which must be admitted: its digest.
async fn written(
    files: &Files,
    reference: &TurnCheckpointRef,
    page_id: &CheckpointOperationId,
    page: CheckpointPage,
) -> Checked<String> {
    let digest = af_core::etag::sha256_hex(&serde_json::to_vec(&page)?);
    let write = Action::PageWrite(TurnCheckpointPageWrite {
        checkpoint: reference.clone(),
        page_id: page_id.clone(),
        digest: digest.clone(),
        contents: page,
    });
    assert!(matches!(
        files.execute(write).await?,
        Response::PageWritten(_)
    ));
    Ok(digest)
}

/// Whether `object` has `verified_at`.
async fn verified(files: &Files, object: &CheckpointObject) -> Checked<bool> {
    let mut connection = files.case.fixtures.database.acquire().await?;
    let verified = sqlx::query_scalar(
        "SELECT verified_at IS NOT NULL FROM runner.checkpoint_objects WHERE id=$1",
    )
    .bind(Uuid7::parse(&object.object_id)?)
    .fetch_one(&mut *connection)
    .await?;
    Ok(verified)
}

/// Every writer pin of the turn ends, as a writer that died after its PUTs.
async fn expire_writers(files: &Files) -> Checked {
    let mut connection = files.case.fixtures.database.acquire().await?;
    sqlx::query(
        "UPDATE runner.checkpoint_pins p SET expires_at = now() - interval '1 minute'
         FROM runner.checkpoints c
         WHERE c.id = p.checkpoint_id AND c.turn_id = $1 AND p.kind = 'writer'",
    )
    .bind(&files.case.acquired.turn_id)
    .execute(&mut *connection)
    .await?;
    Ok(())
}
