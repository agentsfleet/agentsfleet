//! A save that resumed its run publishes beside it: the next model call goes
//! out while the publication is held, a boundary meanwhile skips, and the
//! run's end waits for the receipt.
#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::pin::Pin;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use af_wire::run::{CheckpointOperationId, RunPayload, TurnResume, TurnWorkspace};
use afr_tools::catalog::UPDATE_PLAN;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

use super::tests::engine;
use crate::engine::{AgentEngine as _, AgentRun, Meter};
use crate::fixture::{Canned, Frames, Script, call, payload, say, unbounded};
use crate::testing::Discard;
use crate::{
    CommittedRecovery, FileRecoveryPermit, Publication, Published, RecoveryBoundarySink,
    RecoveryReceipt, RecoverySave, RecoverySkip, RecoverySnapshot, RecoveryState,
};

/// A capture owner whose saves resume the run at once and hold their
/// receipt until the test lets it go.
#[derive(Debug, Default)]
struct Holding {
    saves: AtomicUsize,
    held: Mutex<Option<(oneshot::Sender<Published>, RecoveryReceipt)>>,
}

impl Holding {
    /// Delivers the held receipt; whether the run was still there to take it.
    fn release(&self) -> bool {
        let (send, receipt) = self.held.lock().unwrap().take().unwrap();
        send.send(Ok(RecoverySave::Committed(Box::new(receipt))))
            .is_ok()
    }
}

#[async_trait::async_trait]
impl RecoveryBoundarySink for Holding {
    fn workspace(&self) -> Result<TurnWorkspace, RecoverySkip> {
        Ok(TurnWorkspace::ApiOnly)
    }

    fn try_reserve(&self, _task: bool) -> Result<Option<FileRecoveryPermit>, RecoverySkip> {
        Ok(None)
    }

    async fn save(&self, snapshot: RecoverySnapshot) -> crate::Result<RecoverySave> {
        self.saves.fetch_add(1, Ordering::SeqCst);
        let RecoveryState::Turn(turn) = snapshot.state else {
            panic!("an ordinary run saves its turn");
        };
        let receipt = RecoveryReceipt {
            run_id: snapshot.run_id,
            turn_id: snapshot.turn_id,
            fencing_token: snapshot.fencing_token,
            checkpoint: CommittedRecovery::Turn(TurnResume {
                checkpoint_id: CheckpointOperationId(
                    "019f0000-0000-7000-8000-0000000000aa".to_owned(),
                ),
                generation: 1,
                format_version: af_wire::run::TURN_CHECKPOINT_FORMAT,
                scope: turn.scope,
                manifest_root: af_core::etag::sha256_hex(b"manifest"),
                history_digest: af_core::etag::sha256_hex(&turn.history),
            }),
        };
        let (send, publication) = Publication::channel();
        *self.held.lock().unwrap() = Some((send, receipt));
        Ok(RecoverySave::Publishing(publication))
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

#[tokio::test]
async fn the_next_model_call_does_not_wait_for_publication() {
    let run = recoverable();
    let sink = Holding::default();
    let script = Script::new([
        vec![call("c1", UPDATE_PLAN.name(), serde_json::json!({}))],
        vec![call("c2", UPDATE_PLAN.name(), serde_json::json!({}))],
        vec![say("done")],
    ]);
    let engine = engine(
        vec![
            Canned::boxed(&UPDATE_PLAN, "planned"),
            Canned::boxed(&UPDATE_PLAN, "planned again"),
        ],
        &script,
    );
    let (frames, stop) = (Frames::default(), CancellationToken::new());
    let events = frames.sink();
    let executor = afr_executor::ExecutorSlot::unavailable(afr_executor::Unavailable::NoSandbox);
    let mint = af_egress::testing::CountingMint::never();
    let meter = Meter::default();
    let driven = engine.run(AgentRun {
        recovery: Some(&sink),
        recovered_durable_task: None,
        recovered_turn: None,
        prepared_durable_task: None,
        run: &run,
        memory: afr_memory::Seed::default(),
        executor: &executor,
        mint: &mint,
        verbs: &afr_tools::CLOSED,
        checkpoint: &Discard,
        inbox: &crate::testing::NoInbox,
        events: &events,
        meter: &meter,
        stop: &stop,
    });
    tokio::pin!(driven);

    held_open(driven.as_mut(), &script, 3).await;
    assert!(sink.release(), "the run dropped its publication");
    let output = driven.await.unwrap();
    drop(frames.taken());

    assert_eq!(output.result.content, "done");
    assert_eq!(
        sink.saves.load(Ordering::SeqCst),
        1,
        "the boundary that found the publication running skipped"
    );
}

/// Drives `driven` until `script` has served `requests` model requests, every
/// one while the first save's receipt is held, then a while longer: the run
/// has answered and still waits on its publication.
async fn held_open(mut driven: Pin<&mut impl Future>, script: &Script, requests: usize) {
    while script.sent().len() < requests {
        tokio::select! {
            _ended = driven.as_mut() => panic!("the run ended before its save's receipt"),
            () = tokio::time::sleep(Duration::from_millis(5)) => {}
        }
    }
    tokio::select! {
        _ended = driven.as_mut() => panic!("the run settled before its save committed"),
        () = tokio::time::sleep(Duration::from_millis(50)) => {}
    }
}
