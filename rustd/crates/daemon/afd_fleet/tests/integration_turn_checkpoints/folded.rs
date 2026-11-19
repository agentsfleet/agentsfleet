//! File checkpoints adopt the exact saved input prefix before any replacement work.

use super::{
    Checked,
    expiry::release_after_expiry,
    publication::{command, root},
    storage::Files,
    valid,
};
use af_core::id::Uuid7;
use af_wire::run::{
    FoldRef, TurnCheckpointAction as Action, TurnCheckpointBegin, TurnCheckpointRef,
    TurnCheckpointResponse as Response, TurnId, TurnResume, TurnWorkspace,
};
use sqlx::Connection as _;

const MESSAGE: &str = "Use staging before publishing";

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn file_recovery_adopts_the_saved_nonempty_prefix_under_the_replacement_fence() -> Checked {
    let mut files = Files::new().await?;
    let (saved, fold) = publish(&files).await?;
    let previous = files.case.issued.run_id.clone();
    files.case.replace().await?;
    let reference = read(&files, saved).await?;
    let Response::Adopted(covered) = files
        .execute(Action::Adopt {
            checkpoint: reference.clone(),
        })
        .await?
    else {
        return Err("adopted exact prefix".into());
    };
    assert_eq!(covered, vec![fold.clone()]);
    let claim = claim(&files, &fold).await?;
    assert_ne!(claim.owner, previous);
    assert_eq!(claim.owner, files.case.issued.run_id);
    assert_eq!(claim.fence, files.case.acquired.fence.as_i64());
    assert!(!claim.settled);
    assert!(matches!(
        files
            .execute(Action::PinRelease {
                checkpoint: reference
            })
            .await?,
        Response::PinReleased
    ));
    files.finish().await;
    Ok(())
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn reader_expiry_during_a_fold_lock_wait_rolls_back_every_adopted_claim() -> Checked {
    let mut files = Files::new().await?;
    let (saved, fold) = publish(&files).await?;
    let previous = files.case.issued.run_id.clone();
    files.case.replace().await?;
    let reference = read(&files, saved).await?;
    let mut connection = files.case.fixtures.database.acquire().await?;
    let deadline: jiff_sqlx::Timestamp = sqlx::query_scalar(
        "UPDATE runner.turn_checkpoint_pins SET expires_at=clock_timestamp()+interval '1400 milliseconds' WHERE id=$1 RETURNING expires_at",
    ).bind(Uuid7::parse(&reference.pin_id.0)?).fetch_one(&mut *connection).await?;
    let mut held = connection.begin().await?;
    let controller: i32 =
        sqlx::query_scalar("SELECT pg_backend_pid() FROM fleet.admissions WHERE id=$1 FOR UPDATE")
            .bind(Uuid7::parse(&fold.event_id.0)?)
            .fetch_one(&mut *held)
            .await?;
    let (result, released) = tokio::join!(
        files.execute(Action::Adopt {
            checkpoint: reference
        }),
        release_after_expiry(&files, held, controller, deadline.to_jiff())
    );
    released?;
    assert!(result.is_err_and(|failure| {
        failure
            .downcast_ref::<afd_fleet::Error>()
            .is_some_and(|failure| failure.code() == af_core::error_code::RUN_CHECKPOINT_CONFLICT)
    }));
    assert_eq!(claim(&files, &fold).await?.owner, previous);
    drop(connection);
    files.finish().await;
    Ok(())
}

async fn publish(files: &Files) -> Checked<(Box<TurnResume>, FoldRef)> {
    let fold = incorporate(files).await?;
    let workspace = TurnWorkspace::Files {
        toolbox_digest: "a".repeat(64),
    };
    let history = history(files, workspace.clone(), &fold)?;
    let Response::Begun(plan) = files
        .execute(Action::Begin(TurnCheckpointBegin {
            expected_head: None,
            format_version: 1,
            workspace,
            history_digest: af_core::etag::sha256_hex(history.history.as_bytes()),
            page_count: 1,
            upload_budget_bytes: 0,
            logical_bytes: 0,
            entry_count: 0,
        }))
        .await?
    else {
        return Err("file plan".into());
    };
    let reference = TurnCheckpointRef {
        checkpoint_id: plan.checkpoint_id,
        pin_id: TurnId(plan.pin.id),
    };
    let manifest = empty_manifest(files, &reference, &plan.page_ids).await?;
    let Response::Committed(saved) = files
        .execute(Action::Commit {
            checkpoint: reference,
            root: history,
            manifest_root: manifest,
            folds: vec![fold.clone()],
        })
        .await?
    else {
        return Err("saved complete file generation".into());
    };
    Ok((saved, fold))
}

async fn empty_manifest(
    files: &Files,
    reference: &TurnCheckpointRef,
    page_ids: &[TurnId],
) -> Checked<String> {
    let page_id = page_ids.first().ok_or("page")?.clone();
    let page = af_wire::task::CheckpointPage {
        page_number: 0,
        entries: Vec::new(),
        history_objects: Vec::new(),
    };
    let digest = af_core::etag::sha256_hex(&serde_json::to_vec(&page)?);
    files
        .execute(Action::PageWrite {
            checkpoint: reference.clone(),
            page_id: page_id.clone(),
            digest: digest.clone(),
            contents: page,
        })
        .await?;
    Ok(af_crypto::checkpoint_digest::manifest_root([(
        page_id.0.as_str(),
        digest.as_str(),
    )]))
}

fn history(
    files: &Files,
    workspace: TurnWorkspace,
    fold: &FoldRef,
) -> Checked<af_wire::run::TurnPrivateRoot> {
    let mut history = root(&files.case, workspace);
    let mut encoded: serde_json::Value = serde_json::from_str(&history.history)?;
    *encoded.pointer_mut("/root/folds").ok_or("root folds")? = serde_json::to_value([&fold])?;
    encoded
        .pointer_mut("/root/messages")
        .and_then(serde_json::Value::as_array_mut)
        .ok_or("root messages")?
        .push(serde_json::json!({"User": MESSAGE}));
    history.history = serde_json::to_string(&encoded)?;
    Ok(history)
}

async fn incorporate(files: &Files) -> Checked<FoldRef> {
    let admissions = afd_turns::Admissions::new(
        files.case.fixtures.database.clone(),
        files.case.fixtures.bus().clone(),
        af_crypto::entropy::Entropy::new(),
    );
    let body = serde_json::json!({"message": MESSAGE}).to_string();
    let actor = afd_turns::view::steer_actor("checkpoint-owner");
    let admitted = admissions
        .admit(afd_turns::Admission {
            producer: afd_turns::Producer::Steer,
            key: afd_turns::Key::Unrepeatable,
            fleet: &files.case.acquired.fleet_id,
            workspace: &files.case.acquired.workspace_id,
            actor: &actor,
            event_type: af_wire::event::EventType::Chat,
            request_json: &body,
            reply: afd_turns::Reply::None,
        })
        .await?;
    let claimed = files
        .case
        .runs
        .claim_folds(
            &files.case.runner,
            &files.case.issued.run_id,
            files.case.acquired.fence.as_u64(),
            0,
        )
        .await?;
    let input = claimed.inputs.first().ok_or("incorporated follow-up")?;
    assert_eq!(claimed.inputs.len(), 1);
    assert_eq!(input.event_id, admitted.stored.id.as_str());
    Ok(FoldRef {
        event_id: TurnId(input.event_id.to_string()),
        sequence: input.sequence,
    })
}

async fn read(files: &Files, saved: Box<TurnResume>) -> Checked<TurnCheckpointRef> {
    let request = valid(command(&files.case, Action::Read { descriptor: saved }))?;
    let Response::Read(read) = files
        .store
        .execute(&files.case.runs, files.case.run(), &request)
        .await?
    else {
        return Err("complete pinned root".into());
    };
    assert!(read.root.history.contains(MESSAGE));
    assert_eq!(read.folds.len(), 1);
    Ok(TurnCheckpointRef {
        checkpoint_id: read.descriptor.checkpoint_id,
        pin_id: TurnId(read.pin.id),
    })
}

#[derive(sqlx::FromRow)]
struct Claim {
    owner: Uuid7,
    fence: i64,
    settled: bool,
}

async fn claim(files: &Files, fold: &FoldRef) -> Checked<Claim> {
    let mut connection = files.case.fixtures.database.acquire().await?;
    Ok(sqlx::query_as("SELECT input_claim_run_id AS owner,input_claim_fence AS fence,settled_at IS NOT NULL AS settled FROM fleet.admissions WHERE id=$1")
        .bind(Uuid7::parse(&fold.event_id.0)?).fetch_one(&mut *connection).await?)
}
