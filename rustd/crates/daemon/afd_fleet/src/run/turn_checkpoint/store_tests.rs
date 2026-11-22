//! What the checkpoint entry points refuse before they reach a datastore: a
//! fence the command and the connection disagree on, a verb sent to the wrong
//! door, and a file capture on a deployment with no file storage.

use std::sync::Arc;

use af_core::error_code;
use af_crypto::secret::Kek;
use af_wire::run::{
    CheckpointOperationId, TURN_CHECKPOINT_FORMAT, TurnCheckpointAction, TurnCheckpointBegin,
    TurnCheckpointCommand, TurnCheckpointPinRelease, TurnCheckpointRef, TurnWorkspace,
};

use super::{TurnCheckpoints, TurnRun};
use crate::run::test_dead;

const FENCE: u64 = 3;

fn operation(seed: u8) -> CheckpointOperationId {
    CheckpointOperationId(test_dead::id(seed).as_str().to_owned())
}

fn begin(workspace: TurnWorkspace) -> TurnCheckpointAction {
    let files = matches!(workspace, TurnWorkspace::Files { .. });
    TurnCheckpointAction::Begin(TurnCheckpointBegin {
        expected_head: None,
        format_version: TURN_CHECKPOINT_FORMAT,
        workspace,
        history_digest: "a".repeat(64),
        page_count: u32::from(files),
        upload_budget_bytes: 0,
        logical_bytes: 0,
        entry_count: 0,
    })
}

fn files() -> TurnWorkspace {
    TurnWorkspace::Files {
        toolbox_digest: "b".repeat(64),
    }
}

type Checked = Result<(), garde::Report>;

fn valid(
    fencing_token: u64,
    action: TurnCheckpointAction,
) -> Result<garde::Valid<TurnCheckpointCommand>, garde::Report> {
    garde::Unvalidated::new(TurnCheckpointCommand {
        fencing_token,
        operation_id: operation(9),
        action,
    })
    .validate()
}

fn store() -> TurnCheckpoints {
    TurnCheckpoints::new(Arc::new(Kek::from_bytes([7; af_crypto::KEY_LEN])))
}

// Value: protects=the daemon's checkpoint key never reaches a log line;
// fails_when=Debug is derived and prints the key or the storage handle;
// why_new=the redacted Debug had no case; seam=Debug of the public store.
#[test]
fn the_store_debugs_without_its_key() {
    let rendered = format!("{:?}", store());
    assert_eq!(rendered, "TurnCheckpoints(private)");
}

// Value: protects=a command signed for one fence is never run under another;
// fails_when=execute stops comparing the command's fence with the presented
// one; why_new=the mismatch arm had no case; seam=TurnCheckpoints::execute over
// datastores nobody answers on, so any query would fail differently.
#[tokio::test]
async fn a_command_for_another_fence_is_stale_before_any_read() -> Checked {
    let (runner, run) = (test_dead::id(1), test_dead::id(2));
    let presented = TurnRun {
        runner_id: &runner,
        run_id: &run,
        fencing_token: FENCE,
    };
    let refused = store()
        .execute(
            &test_dead::runs(),
            presented,
            &valid(FENCE + 1, begin(TurnWorkspace::ApiOnly))?,
        )
        .await
        .err()
        .map(|fault| fault.code());
    assert_eq!(refused, Some(error_code::RUN_STALE_FENCING_TOKEN));
    Ok(())
}

// Value: protects=a file capture never starts where no file storage exists;
// fails_when=Begin stops asking for the file backend before allocating;
// why_new=the missing-backend arm had no case; seam=TurnCheckpoints::execute
// with no storage, over datastores nobody answers on.
#[tokio::test]
async fn a_file_capture_without_file_storage_is_unavailable() -> Checked {
    let (runner, run) = (test_dead::id(1), test_dead::id(2));
    let presented = TurnRun {
        runner_id: &runner,
        run_id: &run,
        fencing_token: FENCE,
    };
    let refused = store()
        .with_storage(None)
        .execute(
            &test_dead::runs(),
            presented,
            &valid(FENCE, begin(files()))?,
        )
        .await
        .err()
        .map(|fault| fault.code());
    assert_eq!(refused, Some(error_code::RUN_CHECKPOINT_UNAVAILABLE));
    Ok(())
}

// Value: protects=allocation runs only for a Begin under the presented fence;
// fails_when=begin_turn_checkpoint drops its fence comparison or accepts
// another verb; why_new=both arms had no case; seam=Runs::begin_turn_checkpoint
// over datastores nobody answers on, so either refusal precedes any query.
#[tokio::test]
async fn allocation_refuses_another_fence_and_any_other_verb() -> Checked {
    let (runner, run) = (test_dead::id(1), test_dead::id(2));
    let presented = TurnRun {
        runner_id: &runner,
        run_id: &run,
        fencing_token: FENCE,
    };
    let runs = test_dead::runs();
    let stale = runs
        .begin_turn_checkpoint(presented, &valid(FENCE + 1, begin(TurnWorkspace::ApiOnly))?)
        .await
        .err()
        .map(|fault| fault.code());
    assert_eq!(stale, Some(error_code::RUN_STALE_FENCING_TOKEN));
    let release = TurnCheckpointAction::PinRelease(TurnCheckpointPinRelease {
        checkpoint: TurnCheckpointRef {
            checkpoint_id: operation(4),
            pin_id: operation(5),
        },
    });
    let other = runs
        .begin_turn_checkpoint(presented, &valid(FENCE, release)?)
        .await
        .err()
        .map(|fault| fault.code());
    assert_eq!(other, Some(error_code::RUN_CHECKPOINT_INCOMPATIBLE));
    Ok(())
}
