//! A commit publishes only the root and manifest its capture began with, under
//! the writer's pin, and only the current head is read or adopted.

use af_core::{error_code, id::Uuid7};
use af_wire::{
    protocol::MESSAGE_BYTES_MAX,
    run::{
        CheckpointOperationId, TurnCheckpointAction as Action, TurnCheckpointAdopt,
        TurnCheckpointCommit, TurnCheckpointRead, TurnCheckpointRef, TurnPrivateRoot,
        TurnWorkspace,
    },
};

use super::{
    Checked,
    custody::{code, committed, reader},
    manifest::Pending,
    publication::root,
    storage::Files,
};

fn other_manifest() -> String {
    "f".repeat(64)
}

// Value: protects=a lost Commit acknowledgement replays only the manifest it
// committed; fails_when=a ready plan's replay stops comparing its manifest;
// why_new=the changed replay had no case; seam=the production Commit replayed
// over live Postgres with another manifest digest.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_committed_generation_replays_only_its_own_manifest() -> Checked {
    let files = Files::new().await?;
    let (pending, saved) = committed(&files, TurnWorkspace::ApiOnly, None).await?;
    assert_eq!(
        code(
            files
                .execute(Action::Commit(TurnCheckpointCommit {
                    checkpoint: pending.reference,
                    root: pending.root,
                    manifest_root: other_manifest(),
                    folds: Vec::new(),
                    uploaded: Vec::new(),
                }))
                .await
        ),
        Some(error_code::RUN_CHECKPOINT_CONFLICT)
    );
    assert_eq!(
        files.case.counts().await?.3,
        Some(Uuid7::parse(&saved.checkpoint_id.0)?)
    );
    files.finish().await;
    Ok(())
}

// Value: protects=an API-only capture publishes no file manifest;
// fails_when=commit stops requiring the empty manifest for an API-only plan;
// why_new=the non-empty manifest had no case; seam=the production Commit over
// live Postgres, then the correct Commit of the same capture.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_api_only_capture_commits_only_the_empty_manifest() -> Checked {
    let files = Files::new().await?;
    let pending = Pending::allocate(&files, &[], TurnWorkspace::ApiOnly, None).await?;
    assert_eq!(
        code(
            files
                .execute(Action::Commit(TurnCheckpointCommit {
                    checkpoint: pending.reference.clone(),
                    root: pending.root.clone(),
                    manifest_root: other_manifest(),
                    folds: Vec::new(),
                    uploaded: Vec::new(),
                }))
                .await
        ),
        Some(error_code::RUN_CHECKPOINT_INCOMPATIBLE)
    );
    assert_eq!(files.case.counts().await?.3, None);
    assert!(matches!(
        pending.commit(&files, &[]).await?,
        af_wire::run::TurnCheckpointResponse::Committed(_)
    ));
    files.finish().await;
    Ok(())
}

// Value: protects=a file manifest publishes only under the digest of its pages;
// fails_when=manifest completion stops comparing the claimed root; why_new=the
// mismatched file manifest had no case; seam=the production Commit of a written
// one-page capture with another manifest digest.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_file_manifest_must_digest_its_pages() -> Checked {
    let files = Files::new().await?;
    let pending = Pending::new(&files, &[Vec::new()]).await?;
    let page_id = pending.plan.page_ids.first().ok_or("page")?.clone();
    pending.write(&files, page_id, 0, Vec::new()).await?;
    assert_eq!(
        code(
            files
                .execute(Action::Commit(TurnCheckpointCommit {
                    checkpoint: pending.reference,
                    root: pending.root,
                    manifest_root: other_manifest(),
                    folds: Vec::new(),
                    uploaded: Vec::new(),
                }))
                .await
        ),
        Some(error_code::RUN_CHECKPOINT_INCOMPATIBLE)
    );
    assert_eq!(files.case.counts().await?.3, None);
    files.finish().await;
    Ok(())
}

// Value: protects=a commit publishes the exact root its Begin digested;
// fails_when=commit stops comparing the root with the begun history digest;
// why_new=the swapped root had no case; seam=the production Commit with a valid
// root whose history differs from the one begun.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_commit_must_carry_the_root_its_capture_began_with() -> Checked {
    let files = Files::new().await?;
    let pending = Pending::allocate(&files, &[], TurnWorkspace::ApiOnly, None).await?;
    let swapped = TurnPrivateRoot {
        history: pending
            .root
            .history
            .replace("accepted-prefix", "another-prefix"),
    };
    assert_ne!(swapped.history, pending.root.history);
    assert_eq!(
        code(
            files
                .execute(Action::Commit(TurnCheckpointCommit {
                    checkpoint: pending.reference,
                    root: swapped,
                    manifest_root: af_crypto::checkpoint_digest::manifest_root(
                        std::iter::empty::<(&str, &str)>()
                    ),
                    folds: Vec::new(),
                    uploaded: Vec::new(),
                }))
                .await
        ),
        Some(error_code::RUN_CHECKPOINT_CONFLICT)
    );
    assert_eq!(files.case.counts().await?.3, None);
    files.finish().await;
    Ok(())
}

// Value: protects=only the writer pin commits; fails_when=commit stops
// requiring writer custody; why_new=a reader-pin commit had no case; seam=the
// production Commit naming a reader pin the production Read granted.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_reader_pin_cannot_commit() -> Checked {
    let files = Files::new().await?;
    let (pending, saved) = committed(&files, TurnWorkspace::ApiOnly, None).await?;
    let held = reader(&files, &saved).await?;
    assert_eq!(
        code(
            files
                .execute(Action::Commit(TurnCheckpointCommit {
                    checkpoint: held,
                    root: pending.root,
                    manifest_root: saved.manifest_root.clone(),
                    folds: Vec::new(),
                    uploaded: Vec::new(),
                }))
                .await
        ),
        Some(error_code::RUN_CHECKPOINT_CONFLICT)
    );
    files.finish().await;
    Ok(())
}

// Value: protects=a generation is published only when its read-back fits one
// message; fails_when=commit drops the read-back size check; why_new=the
// oversized root had no case; seam=the production Begin and Commit of a root at
// the root's own byte bound, leaving no room for the read envelope.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_root_too_large_to_read_back_is_a_limit() -> Checked {
    let files = Files::new().await?;
    let base_root = root(&files.case, TurnWorkspace::ApiOnly);
    let mut encoded: serde_json::Value = serde_json::from_str(&base_root.history)?;
    let messages = encoded
        .pointer_mut("/root/messages")
        .and_then(serde_json::Value::as_array_mut)
        .ok_or("root messages")?;
    messages.push(serde_json::json!({"User": ""}));
    let base = serde_json::to_string(&encoded)?.len();
    let padded = encoded
        .pointer_mut("/root/messages/1/User")
        .ok_or("padded message")?;
    *padded = serde_json::Value::String("p".repeat(MESSAGE_BYTES_MAX - base));
    let large = TurnPrivateRoot {
        history: serde_json::to_string(&encoded)?,
    };
    assert_eq!(large.history.len(), MESSAGE_BYTES_MAX);

    let mut begin = files.case.command();
    if let Action::Begin(input) = &mut begin.action {
        input.history_digest = af_core::etag::sha256_hex(large.history.as_bytes());
    }
    let af_wire::run::TurnCheckpointResponse::Begun(plan) = files.execute(begin.action).await?
    else {
        return Err("large capture".into());
    };
    let checkpoint = TurnCheckpointRef {
        checkpoint_id: plan.checkpoint_id,
        pin_id: CheckpointOperationId(plan.pin.id),
    };
    assert_eq!(
        code(
            files
                .execute(Action::Commit(TurnCheckpointCommit {
                    checkpoint,
                    root: large,
                    manifest_root: af_crypto::checkpoint_digest::manifest_root(
                        std::iter::empty::<(&str, &str)>()
                    ),
                    folds: Vec::new(),
                    uploaded: Vec::new(),
                }))
                .await
        ),
        Some(error_code::RUN_CHECKPOINT_LIMIT)
    );
    assert_eq!(files.case.counts().await?.3, None);
    files.finish().await;
    Ok(())
}

// Value: protects=a restorer reads and adopts only the current head, and adopts
// only with a reader pin; fails_when=Read stops comparing the head, Adopt stops
// comparing the head, or Adopt accepts a writer pin; why_new=the three refusals
// had no case; seam=the production Read and Adopt after the next generation
// replaced the one first read.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn only_the_current_head_is_read_or_adopted() -> Checked {
    let files = Files::new().await?;
    let (_, first) = committed(&files, TurnWorkspace::ApiOnly, None).await?;
    let held = reader(&files, &first).await?;
    let (second_pending, second) = committed(
        &files,
        TurnWorkspace::ApiOnly,
        Some(first.checkpoint_id.clone()),
    )
    .await?;
    assert_eq!(
        code(
            files
                .execute(Action::Read(TurnCheckpointRead {
                    descriptor: first.clone(),
                }))
                .await
        ),
        Some(error_code::RUN_CHECKPOINT_INCOMPATIBLE)
    );
    assert_eq!(
        code(
            files
                .execute(Action::Adopt(TurnCheckpointAdopt { checkpoint: held }))
                .await
        ),
        Some(error_code::RUN_CHECKPOINT_CONFLICT)
    );
    assert_eq!(
        code(
            files
                .execute(Action::Adopt(TurnCheckpointAdopt {
                    checkpoint: second_pending.reference,
                }))
                .await
        ),
        Some(error_code::RUN_CHECKPOINT_CONFLICT)
    );
    assert_eq!(
        files.case.counts().await?.3,
        Some(Uuid7::parse(&second.checkpoint_id.0)?)
    );
    files.finish().await;
    Ok(())
}
