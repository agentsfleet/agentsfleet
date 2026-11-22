//! A page is frozen once, at its preallocated identity, holding only sorted
//! full-manifest entries over verified chunks.

use af_core::error_code;
use af_wire::{
    run::{
        CheckpointOperationId, TurnCheckpointAction as Action, TurnCheckpointRef,
        TurnCheckpointResponse as Response,
    },
    task::{CheckpointEntry, CheckpointFile, CheckpointPage, CheckpointPlaintext},
};
use afd_db::test_util::mint_id;

use super::{
    Checked,
    custody::code,
    files::{CONTENT, begin},
    manifest::Pending,
    storage::Files,
};

fn page(entries: Vec<CheckpointEntry>) -> CheckpointPage {
    CheckpointPage {
        page_number: 0,
        entries,
        history_objects: Vec::new(),
    }
}

fn directory(path: &str) -> CheckpointEntry {
    CheckpointEntry::Directory {
        path: path.to_owned(),
    }
}

async fn write(
    files: &Files,
    checkpoint: &TurnCheckpointRef,
    page_id: &CheckpointOperationId,
    contents: CheckpointPage,
) -> Checked<Response> {
    files
        .execute(Action::PageWrite {
            checkpoint: checkpoint.clone(),
            page_id: page_id.clone(),
            digest: af_core::etag::sha256_hex(&serde_json::to_vec(&contents)?),
            contents,
        })
        .await
}

// Value: protects=a page freezes once, at a page the plan allocated, with only
// sorted full-manifest entries; fails_when=page validation drops the history,
// order or removal refusal, accepts a page id the plan never allocated, lets a
// replay change a frozen page, or refuses a replay of the same bytes;
// why_new=those arms had no case; seam=the production PageWrite over live
// Postgres and the private storage fixture.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_page_freezes_once_with_sorted_full_manifest_entries() -> Checked {
    let files = Files::new().await?;
    let pending = Pending::new(&files, &[Vec::new()]).await?;
    let reference = &pending.reference;
    let page_id = pending.plan.page_ids.first().ok_or("page")?.clone();
    let history = CheckpointPage {
        history_objects: vec![mint_id().as_str().to_owned()],
        ..page(Vec::new())
    };
    let disordered = page(vec![directory("b"), directory("a")]);
    let removal = page(vec![CheckpointEntry::Remove {
        path: "a".to_owned(),
    }]);
    for refused in [history, disordered, removal] {
        assert_eq!(
            code(write(&files, reference, &page_id, refused).await),
            Some(error_code::RUN_CHECKPOINT_INCOMPATIBLE)
        );
    }
    let unallocated = CheckpointOperationId(mint_id().as_str().to_owned());
    assert_eq!(
        code(write(&files, reference, &unallocated, page(Vec::new())).await),
        Some(error_code::RUN_CHECKPOINT_CONFLICT)
    );

    let frozen = page(vec![directory("d")]);
    for _ in 0..2 {
        let Response::PageWritten(written) =
            write(&files, reference, &page_id, frozen.clone()).await?
        else {
            return Err("the page freezes, and its replay answers the same".into());
        };
        assert_eq!(written.entries.len(), 1);
    }
    assert_eq!(
        code(write(&files, reference, &page_id, page(vec![directory("e")])).await),
        Some(error_code::RUN_CHECKPOINT_CONFLICT)
    );
    files.finish().await;
    Ok(())
}

// Value: protects=a manifest names only chunks the daemon has authenticated;
// fails_when=page validation stops requiring a named chunk be verified;
// why_new=the unverified chunk had no case; seam=the production Objects
// registration with no upload, then PageWrite naming that chunk.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_page_naming_an_unverified_chunk_is_incompatible() -> Checked {
    let files = Files::new().await?;
    let (plan, _) = begin(&files).await?;
    let reference = TurnCheckpointRef {
        checkpoint_id: plan.checkpoint_id.clone(),
        pin_id: CheckpointOperationId(plan.pin.id.clone()),
    };
    let Response::Objects(mut registered) = files
        .execute(Action::Objects {
            checkpoint: reference.clone(),
            objects: vec![CheckpointPlaintext {
                sha256: af_core::etag::sha256_hex(CONTENT),
                size: u64::try_from(CONTENT.len())?,
                kind: "file".into(),
            }],
        })
        .await?
    else {
        return Err("registered object".into());
    };
    let object = registered.pop().ok_or("object")?;
    let named = page(vec![CheckpointEntry::File(CheckpointFile {
        path: "state.txt".into(),
        size: u64::try_from(CONTENT.len())?,
        executable: false,
        objects: vec![object.object_id],
    })]);
    let page_id = plan.page_ids.first().ok_or("page")?;
    assert_eq!(
        code(write(&files, &reference, page_id, named).await),
        Some(error_code::RUN_CHECKPOINT_INCOMPATIBLE)
    );
    files.finish().await;
    Ok(())
}
