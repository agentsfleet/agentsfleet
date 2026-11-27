//! A publication the bucket fails mid-upload commits nothing: the head stays
//! the previous generation, and the next save is a delta on it.
//!
//! ```text
//!   G1  full  {state-0, state-1}            the head
//!   G2  delta on G1, its third PUT refused  abandoned, never the head
//!   G3  delta on G1, the same changes       the head; G1's tree with them
//! ```
//!
//! G3 follows once G2's writer pin has lapsed, which is when the daemon gives
//! back the content G2 registered.

use std::collections::BTreeMap;

use super::{
    Checked,
    key_fixture::{open, save},
    manifest::Pending,
    no_read_back::expire_writers,
    publication::root,
    storage::{self, Files},
};
use af_chunk_store::Resolved;
use af_core::id::Uuid7;
use af_wire::{
    durable_task::{
        CheckpointEntry, CheckpointFile, CheckpointObject, CheckpointObjectStatus as Status,
        CheckpointPage, CheckpointPlaintext, CheckpointTransferPurpose, CheckpointUpload,
    },
    run::{
        CheckpointOperationId, TurnCheckpointAbandon, TurnCheckpointAction as Action,
        TurnCheckpointAuthorize, TurnCheckpointBegin, TurnCheckpointCommit, TurnCheckpointObjects,
        TurnCheckpointPageRead, TurnCheckpointPinRelease, TurnCheckpointPlan, TurnCheckpointRead,
        TurnCheckpointRef, TurnCheckpointResponse as Response, TurnCheckpointSaved,
        TurnCheckpointUploads, TurnResume, TurnWorkspace,
    },
};
use serde::de::DeserializeOwned;

/// A workspace's files, by path.
type Tree = BTreeMap<String, Vec<u8>>;

/// What the first save holds, as `key_fixture::save` names its files.
const BASE: [&[u8]; 2] = [b"kept state", b"first state"];
/// What the run wrote after it: one file rewritten and three added.
const CHANGED: [(&str, &[u8]); 4] = [
    ("state-1.txt", b"second state"),
    ("state-2.txt", b"added state a"),
    ("state-3.txt", b"added state b"),
    ("state-4.txt", b"added state c"),
];

fn tree(files: &[(&str, &[u8])]) -> Tree {
    let named = files.iter();
    named
        .map(|(path, content)| ((*path).to_owned(), content.to_vec()))
        .collect()
}

/// What the daemon answers `action` with, read as the answer of `kind` it
/// must be: the wire's own spelling, so an answer of any other kind fails
/// here.
async fn answered<T: DeserializeOwned>(files: &Files, action: Action, kind: &str) -> Checked<T> {
    let mut answer = serde_json::to_value(files.execute(action).await?)?;
    let answers = answer.get("kind").and_then(serde_json::Value::as_str);
    assert_eq!(answers, Some(kind), "the daemon answered another kind");
    let carried = answer.get_mut("value").map(serde_json::Value::take);
    Ok(serde_json::from_value(
        carried.ok_or("the answer's value")?,
    )?)
}

/// A delta plan on `head` for [`CHANGED`], which resolves the tree to `resolved`.
async fn begun(files: &Files, head: &TurnResume, resolved: &Tree) -> Checked<Pending> {
    let workspace = TurnWorkspace::Files {
        toolbox_digest: "a".repeat(64),
        sandbox: super::sandbox(),
    };
    let history = root(&files.case, workspace.clone());
    let changed: usize = CHANGED.iter().map(|(_, content)| content.len()).sum();
    let begin = Action::Begin(TurnCheckpointBegin {
        expected_head: Some(head.checkpoint_id.clone()),
        parent: Some(head.checkpoint_id.clone()),
        format_version: 1,
        workspace,
        history_digest: af_core::etag::sha256_hex(history.history.as_bytes()),
        page_count: 1,
        upload_budget_bytes: u64::try_from(changed + CHANGED.len() * af_crypto::TAG_LEN)?,
        logical_bytes: u64::try_from(resolved.values().map(Vec::len).sum::<usize>())?,
        entry_count: u32::try_from(resolved.len())?,
    });
    let plan: TurnCheckpointPlan = answered(files, begin, "begun").await?;
    Ok(Pending {
        reference: TurnCheckpointRef {
            checkpoint_id: plan.checkpoint_id.clone(),
            pin_id: CheckpointOperationId(plan.pin.id.clone()),
        },
        plan,
        root: history,
    })
}

/// Registers [`CHANGED`] under `pending`, each chunk new to the turn, and
/// PUTs them in order until the bucket refuses one: every registration, and
/// the objects whose PUT was accepted.
async fn uploaded(
    files: &Files,
    pending: &Pending,
) -> Checked<(Vec<CheckpointObject>, Vec<CheckpointOperationId>)> {
    let mut descriptors = Vec::new();
    for (_, content) in CHANGED {
        descriptors.push(CheckpointPlaintext {
            sha256: af_core::etag::sha256_hex(content),
            size: u64::try_from(content.len())?,
            kind: "file".into(),
        });
    }
    let registered = Action::Objects(TurnCheckpointObjects {
        checkpoint: pending.reference.clone(),
        objects: descriptors,
    });
    let objects: Vec<CheckpointObject> = answered(files, registered, "objects").await?;
    let missing = |object: &CheckpointObject| matches!(object.status, Status::Missing);
    assert!(objects.iter().all(missing), "each chunk is new to the turn");
    let mut accepted = Vec::new();
    for ((_, content), object) in CHANGED.iter().zip(&objects) {
        if !put(files, &pending.reference, object, content).await? {
            break;
        }
        accepted.push(CheckpointOperationId(object.object_id.clone()));
    }
    Ok((objects, accepted))
}

/// Seals `content` as `object` and PUTs it over its signed link: whether the
/// bucket accepted it.
async fn put(
    files: &Files,
    reference: &TurnCheckpointRef,
    object: &CheckpointObject,
    content: &[u8],
) -> Checked<bool> {
    let sealed = storage::encrypt(&files.case.acquired.workspace_id, object, content)?;
    let frozen = Action::Uploads(TurnCheckpointUploads {
        checkpoint: reference.clone(),
        objects: vec![CheckpointUpload {
            object_id: object.object_id.clone(),
            ciphertext_sha256: af_core::etag::sha256_hex(&sealed),
            ciphertext_length: u64::try_from(sealed.len())?,
        }],
    });
    let mut signed: Vec<CheckpointObject> = answered(files, frozen, "uploads").await?;
    let transfer = signed.pop().and_then(|object| object.transfer);
    let transfer = transfer.ok_or("create-only transfer")?;
    let mut request = files.server.client.put(&transfer.url);
    for (name, value) in &transfer.headers {
        request = request.header(name, value);
    }
    Ok(request.body(sealed).send().await?.status().is_success())
}

/// Writes `pending`'s one page, [`CHANGED`] held by `objects`, and commits
/// it reporting `accepted`.
async fn committed(
    files: &Files,
    pending: &Pending,
    objects: &[CheckpointObject],
    accepted: Vec<CheckpointOperationId>,
) -> Checked<TurnResume> {
    let mut entries = Vec::new();
    for ((path, content), object) in CHANGED.iter().zip(objects) {
        entries.push(CheckpointEntry::File(CheckpointFile {
            path: (*path).to_owned(),
            size: u64::try_from(content.len())?,
            executable: false,
            objects: vec![object.object_id.clone()],
        }));
    }
    let page = pending.plan.page_ids.first().ok_or("page")?.clone();
    let (id, digest) = pending.write(files, page, 0, entries).await?;
    let commit = Action::Commit(TurnCheckpointCommit {
        checkpoint: pending.reference.clone(),
        root: pending.root.clone(),
        manifest_root: af_crypto::checkpoint_digest::manifest_root([(
            id.as_str(),
            digest.as_str(),
        )]),
        folds: Vec::new(),
        uploaded: accepted,
    });
    answered(files, commit, "committed").await
}

/// What the current run is offered and restores, as a replacement does: the
/// generation's identifier, and its chain resolved with every file's objects
/// fetched from the bucket and opened.
async fn restored(files: &Files) -> Checked<(String, Tree)> {
    let offered = files.store.resume(&files.case.runs, files.case.run());
    let descriptor = offered.await?.ok_or("an offered checkpoint")?;
    let read = Action::Read(TurnCheckpointRead {
        descriptor: Box::new(descriptor.clone()),
    });
    let saved: TurnCheckpointSaved = answered(files, read, "read").await?;
    let reference = TurnCheckpointRef {
        checkpoint_id: descriptor.checkpoint_id.clone(),
        pin_id: CheckpointOperationId(saved.pin.id.clone()),
    };
    let links = saved.chain.iter().flat_map(|link| link.page_ids.iter());
    let pages = links.cloned().map(CheckpointOperationId);
    let mut resolved = Resolved::default();
    for page_id in pages.chain(saved.page_ids.iter().cloned()) {
        let checkpoint = reference.clone();
        let read = Action::PageRead(TurnCheckpointPageRead {
            checkpoint,
            page_id,
        });
        let page: CheckpointPage = answered(files, read, "page_read").await?;
        page.entries
            .into_iter()
            .try_for_each(|entry| resolved.apply(entry))?;
    }
    let mut tree = Tree::new();
    for (path, entry) in resolved.entries() {
        if let CheckpointEntry::File(file) = entry {
            tree.insert(path.clone(), fetched(files, &reference, file).await?);
        }
    }
    let checkpoint = reference;
    let released = Action::PinRelease(TurnCheckpointPinRelease { checkpoint });
    assert!(matches!(
        files.execute(released).await?,
        Response::PinReleased
    ));
    Ok((descriptor.checkpoint_id.0, tree))
}

/// `file`'s bytes: each of its objects authorized for restore, fetched from
/// the bucket and opened, in order.
async fn fetched(
    files: &Files,
    reference: &TurnCheckpointRef,
    file: &CheckpointFile,
) -> Checked<Vec<u8>> {
    let authorize = Action::Authorize(TurnCheckpointAuthorize {
        checkpoint: reference.clone(),
        purpose: CheckpointTransferPurpose::Restore,
        object_ids: file
            .objects
            .iter()
            .cloned()
            .map(CheckpointOperationId)
            .collect(),
    });
    let frames: Vec<CheckpointObject> = answered(files, authorize, "authorized").await?;
    let mut bytes = Vec::new();
    for id in &file.objects {
        let frame = frames.iter().find(|frame| frame.object_id == *id);
        let frame = frame.ok_or("the object's restore frame")?;
        let url = &frame.transfer.as_ref().ok_or("download link")?.url;
        let stored = files.server.client.get(url).send().await?;
        let stored = stored.error_for_status()?.bytes().await?.to_vec();
        bytes.extend(open(&files.case.acquired.workspace_id, frame, stored)?);
    }
    Ok(bytes)
}

/// Every generation of the turn, oldest first: its identifier, whether it is
/// ready, whether it is abandoned, and the parent it is a delta on.
async fn generations(files: &Files) -> Checked<Vec<(String, bool, bool, Option<String>)>> {
    let mut connection = files.case.fixtures.database.acquire().await?;
    let rows: Vec<(Uuid7, bool, bool, Option<Uuid7>)> = sqlx::query_as(
        "SELECT id, ready_at IS NOT NULL, abandoned_at IS NOT NULL, parent_checkpoint_id
         FROM runner.checkpoints WHERE turn_id=$1 ORDER BY generation",
    )
    .bind(&files.case.acquired.turn_id)
    .fetch_all(&mut *connection)
    .await?;
    let spelled = rows.into_iter().map(|(id, ready, abandoned, parent)| {
        let parent = parent.map(|parent| parent.to_string());
        (id.to_string(), ready, abandoned, parent)
    });
    Ok(spelled.collect())
}

// Value: protects=a publication whose third PUT the bucket refuses leaves
// the head on the previous generation, and the save after it is a delta on
// that generation which restores byte for byte; fails_when=the failed
// generation becomes ready or the head, the next save names the failed one
// as its parent or cannot store the content it lost, or a restore differs by
// one byte; why_new=the supervisor's case of this failure answers from a
// fake daemon; seam=the production turn verbs over live Postgres and real
// SDK requests to the private HTTPS fixture.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_publication_failing_after_resume_keeps_the_previous_generation() -> Checked {
    let files = Files::new().await?;
    let (previous, _) = save(&files, &BASE, None).await?;
    let before = tree(&[("state-0.txt", BASE[0]), ("state-1.txt", BASE[1])]);
    let mut after = before.clone();
    after.extend(tree(&CHANGED));
    let head = previous.checkpoint_id.0.clone();
    files.server.refuse_put(3);

    let failed = begun(&files, &previous, &after).await?;
    let (_, accepted) = uploaded(&files, &failed).await?;
    let checkpoint = failed.reference.clone();
    let abandoned = Action::Abandon(TurnCheckpointAbandon { checkpoint });
    assert!(matches!(
        files.execute(abandoned).await?,
        Response::Abandoned
    ));

    assert_eq!(accepted.len(), 2, "the third PUT was refused");
    let pending = failed.plan.checkpoint_id.0.clone();
    assert_eq!(
        generations(&files).await?,
        [
            (head.clone(), true, false, None),
            (pending.clone(), false, true, Some(head.clone())),
        ]
    );
    assert_eq!(files.case.counts().await?.3, Some(Uuid7::parse(&head)?));
    assert_eq!(restored(&files).await?, (head.clone(), before));

    // The daemon gives back the content the failed save registered once
    // that save's writer pin lapses; until then it answers it busy.
    expire_writers(&files).await?;
    let next = begun(&files, &previous, &after).await?;
    let (objects, accepted) = uploaded(&files, &next).await?;
    let saved = committed(&files, &next, &objects, accepted).await?;

    let rows = generations(&files).await?;
    let delta = (saved.checkpoint_id.0.clone(), true, false, Some(head));
    assert_eq!(
        rows.last(),
        Some(&delta),
        "a delta on the previous generation"
    );
    assert!(!rows.iter().any(|(id, ready, ..)| *id == pending && *ready));
    assert_eq!(restored(&files).await?, (saved.checkpoint_id.0, after));
    files.finish().await;
    Ok(())
}
