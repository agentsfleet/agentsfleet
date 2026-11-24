//! An ordinary root saves its checkpoint at the boundary before each model
//! call after the first: a skip or a refused reservation leaves the run
//! answering, and a receipt for another attempt or a failed cleanup stops it.
#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::sync::atomic::{AtomicUsize, Ordering};

use af_wire::run::{RunPayload, TurnWorkspace};
use afr_tools::catalog::UPDATE_PLAN;
use tokio_util::sync::CancellationToken;

use super::finish::DETAIL_STOPPED;
use super::tests::engine;
use crate::engine::{AgentEngine as _, AgentRun, Meter, RunOutput};
use crate::fixture::{Canned, Frames, Script, call, payload, say, unbounded};
use crate::testing::Discard;
use crate::{
    CommittedRecovery, FileRecoveryPermit, RecoveryBoundarySink, RecoveryReceipt, RecoverySave,
    RecoverySkip, RecoverySnapshot, ResultOutcome,
};

/// What the scripted capture owner answers a save with.
#[derive(Debug, Clone, Copy)]
enum Answer {
    /// No new generation: the executor was busy.
    Skipped,
    /// A committed receipt for another attempt.
    Elsewhere,
    /// Cleanup failed after the capture.
    Failed,
}

/// A capture owner whose workspace is ready, whose reservation refuses with
/// `refusal` when set, and whose saves answer `answer`.
#[derive(Debug)]
struct Sink {
    refusal: Option<RecoverySkip>,
    answer: Answer,
    saves: AtomicUsize,
}

impl Sink {
    const fn new(refusal: Option<RecoverySkip>, answer: Answer) -> Self {
        Self {
            refusal,
            answer,
            saves: AtomicUsize::new(0),
        }
    }
}

#[async_trait::async_trait]
impl RecoveryBoundarySink for Sink {
    fn workspace(&self) -> Result<TurnWorkspace, RecoverySkip> {
        Ok(TurnWorkspace::ApiOnly)
    }

    fn try_reserve(&self, _task: bool) -> Result<Option<FileRecoveryPermit>, RecoverySkip> {
        self.refusal.map_or(Ok(None), Err)
    }

    async fn save(&self, snapshot: RecoverySnapshot) -> crate::Result<RecoverySave> {
        self.saves.fetch_add(1, Ordering::SeqCst);
        match self.answer {
            Answer::Skipped => Ok(RecoverySave::Skipped(RecoverySkip::BusyExecutor)),
            Answer::Elsewhere => Ok(RecoverySave::Committed(Box::new(RecoveryReceipt {
                run_id: "019f0000-0000-7000-8000-0000000000ff".to_owned(),
                turn_id: snapshot.turn_id,
                fencing_token: snapshot.fencing_token,
                checkpoint: CommittedRecovery::DurableTask {
                    checkpoint_id: "019f0000-0000-7000-8000-0000000000fe".to_owned(),
                    generation: 1,
                    history_digest: "0".repeat(64),
                },
            }))),
            Answer::Failed => Err(crate::error::recovery_invalid(
                "the capture owner could not thaw the workspace",
            )),
        }
    }
}

/// An ordinary run that may be recovered: its identities are the ones a
/// checkpoint scope carries.
fn recoverable() -> RunPayload<'static> {
    let id = |number: u32| format!("019f0000-0000-7000-8000-{number:012x}");
    let mut run = payload(&[UPDATE_PLAN.name()], unbounded());
    run.event.workspace_id = id(1).into();
    run.event.fleet_id = id(2).into();
    run.event.turn_id = id(3).into();
    run.run_id = id(5).into();
    run
}

/// `run` driven to its end over a call and an answer, saving through `sink`.
async fn drive(run: &RunPayload<'_>, sink: &Sink) -> RunOutput {
    let script = Script::new([
        vec![call("c1", UPDATE_PLAN.name(), serde_json::json!({}))],
        vec![say("done")],
    ]);
    let engine = engine(vec![Canned::boxed(&UPDATE_PLAN, "planned")], &script);
    let frames = Frames::default();
    let events = frames.sink();
    let stop = CancellationToken::new();
    let output = engine
        .run(AgentRun {
            recovery: Some(sink),
            recovered_durable_task: None,
            recovered_turn: None,
            prepared_durable_task: None,
            run,
            memory: afr_memory::Seed::default(),
            executor: &afr_executor::ExecutorSlot::unavailable(
                afr_executor::Unavailable::NoSandbox,
            ),
            mint: &af_egress::testing::CountingMint::never(),
            verbs: &afr_tools::CLOSED,
            checkpoint: &Discard,
            inbox: &crate::testing::NoInbox,
            events: &events,
            meter: &Meter::default(),
            stop: &stop,
        })
        .await;
    drop(frames.taken());
    output.unwrap()
}

#[tokio::test]
async fn a_skipped_or_unreserved_boundary_leaves_the_run_answering() {
    for (sink, saves) in [
        (Sink::new(None, Answer::Skipped), 1),
        (
            Sink::new(Some(RecoverySkip::BusyStorage), Answer::Skipped),
            0,
        ),
    ] {
        let output = drive(&recoverable(), &sink).await;

        assert_eq!(output.result.content, "done");
        assert_eq!(sink.saves.load(Ordering::SeqCst), saves);
    }
}

#[tokio::test]
async fn a_root_that_cannot_be_captured_skips_its_checkpoint_and_answers() {
    let mut run = recoverable();
    // A scope that names no fleet identifier is one no checkpoint can carry.
    run.event.fleet_id = "fleet-1".into();
    let sink = Sink::new(None, Answer::Skipped);

    let output = drive(&run, &sink).await;

    assert_eq!(output.result.content, "done");
    assert_eq!(sink.saves.load(Ordering::SeqCst), 0, "nothing was captured");
}

#[tokio::test]
async fn a_receipt_for_another_attempt_or_a_failed_cleanup_stops_the_run() {
    for answer in [Answer::Elsewhere, Answer::Failed] {
        let sink = Sink::new(None, answer);

        let output = drive(&recoverable(), &sink).await;

        let ResultOutcome::Failed(failure) = output.result.outcome else {
            panic!("{answer:?} ended {:?}", output.result.outcome);
        };
        assert_eq!(failure.detail, DETAIL_STOPPED, "{answer:?}");
        assert_eq!(sink.saves.load(Ordering::SeqCst), 1, "{answer:?}");
        assert!(output.result.content.is_empty(), "{answer:?}");
    }
}

#[tokio::test]
async fn an_assigned_checkpoint_with_no_restored_state_refuses_the_run_before_any_model_call() {
    use af_wire::run::{CheckpointOperationId, TurnResume, TurnScope};

    let id = |number: u32| CheckpointOperationId(format!("019f0000-0000-7000-8000-{number:012x}"));
    let mut run = recoverable();
    run.turn_resume = Some(TurnResume {
        checkpoint_id: id(6),
        generation: 1,
        format_version: 1,
        scope: TurnScope {
            workspace_id: id(1),
            fleet_id: id(2),
            turn_id: id(3),
            workspace: TurnWorkspace::ApiOnly,
        },
        manifest_root: af_core::etag::sha256_hex(b"manifest"),
        history_digest: af_core::etag::sha256_hex(b"history"),
    });
    let script = Script::new([vec![say("done")]]);
    let engine = engine(vec![], &script);
    let frames = Frames::default();
    let events = frames.sink();
    let stop = CancellationToken::new();

    let refused = engine
        .run(AgentRun {
            recovery: None,
            recovered_durable_task: None,
            recovered_turn: None,
            prepared_durable_task: None,
            run: &run,
            memory: afr_memory::Seed::default(),
            executor: &afr_executor::ExecutorSlot::unavailable(
                afr_executor::Unavailable::NoSandbox,
            ),
            mint: &af_egress::testing::CountingMint::never(),
            verbs: &afr_tools::CLOSED,
            checkpoint: &Discard,
            inbox: &crate::testing::NoInbox,
            events: &events,
            meter: &Meter::default(),
            stop: &stop,
        })
        .await
        .err()
        .unwrap();

    assert_eq!(
        refused.code(),
        af_core::error_code::RUN_CHECKPOINT_INCOMPATIBLE
    );
    assert!(
        refused
            .to_string()
            .contains("assigned recovery has no single complete restore owner"),
        "{refused}"
    );
    assert!(script.sent().is_empty(), "the model was called");
    assert!(frames.taken().is_empty(), "a refused run sends no frame");
}
