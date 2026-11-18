//! A durable commit outlives transport retries and the next capture boundary.

use super::*;
use af_wire::lease::{
    TurnCheckpointAction as Action, TurnCheckpointCommand, TurnCheckpointPlan,
    TurnCheckpointResponse as Response, TurnId, TurnResume, TurnScope, TurnWorkspace,
};
use afr_agent::TurnSnapshot;
use zeroize::Zeroizing;

#[tokio::test(start_paused = true)]
async fn losing_every_commit_ack_does_not_permanently_stall_later_captures() -> crate::Result<()> {
    let ledger = Arc::new(Mutex::new(Ledger::default()));
    let remote = Arc::clone(&ledger);
    let (plane, mut calls) = plane(move |call| remote.lock().unwrap().respond(body(call)));
    let lease = Uuid7::parse(ID)?;
    let mut native = super::super::NativeTurn::new(&plane, &lease, FENCE);
    let stop = tokio_util::sync::CancellationToken::new();
    assert!(
        native
            .capture(snapshot(), None, &stop)
            .await
            .is_err_and(|failure| failure.is_retryable())
    );
    confirm_commit_and_allow_acks(&ledger);
    let saved = native.capture(snapshot(), None, &stop).await?;
    assert_eq!(saved.generation, 2);
    assert_eq!(ledger.lock().unwrap().allocated, 2);
    assert_eq!(ledger.lock().unwrap().head.as_ref(), Some(&saved));
    assert!(
        drained(&mut calls)
            .iter()
            .all(|call| call.verb == Verb::TurnCheckpoint)
    );
    Ok(())
}

fn confirm_commit_and_allow_acks(ledger: &Arc<Mutex<Ledger>>) {
    let mut saved = ledger.lock().unwrap();
    assert_eq!(saved.head.as_ref().unwrap().generation, 1);
    assert!(saved.commit_operations.len() > 1);
    assert!(
        saved
            .commit_operations
            .iter()
            .all(|operation| Some(operation) == saved.commit_operations.first())
    );
    saved.lose_ack = false;
}

fn drained(calls: &mut Calls) -> Vec<Call> {
    crate::test_support::drain(calls)
}

fn snapshot() -> TurnSnapshot {
    TurnSnapshot {
        scope: TurnScope {
            workspace_id: TurnId(ID.to_owned()),
            fleet_id: TurnId(ID.to_owned()),
            event_id: TurnId(ID.to_owned()),
            setup_revision_id: TurnId(ID.to_owned()),
            workspace: TurnWorkspace::ApiOnly,
        },
        previous: None,
        history: Zeroizing::new(b"private root".to_vec()),
        folds: Vec::new(),
    }
}

struct Ledger {
    head: Option<TurnResume>,
    plan: Option<TurnCheckpointPlan>,
    allocated: u64,
    lose_ack: bool,
    commit_operations: Vec<TurnId>,
}

impl Default for Ledger {
    fn default() -> Self {
        Self {
            head: None,
            plan: None,
            allocated: 0,
            lose_ack: true,
            commit_operations: Vec::new(),
        }
    }
}

impl Ledger {
    fn respond(&mut self, request: TurnCheckpointCommand) -> Answer {
        match request.action {
            Action::Begin(begin) => self.begin(begin),
            Action::Commit {
                checkpoint,
                root,
                manifest_root,
                folds,
            } => {
                assert_eq!(folds, Vec::<af_wire::lease::FoldRef>::new());
                let plan = self.plan.as_ref().unwrap();
                assert_eq!(checkpoint.checkpoint_id, plan.checkpoint_id);
                assert_eq!(
                    af_core::etag::sha256_hex(root.history.as_bytes()),
                    plan.history_digest
                );
                self.head = Some(TurnResume {
                    checkpoint_id: plan.checkpoint_id.clone(),
                    generation: plan.generation,
                    format_version: plan.format_version,
                    scope: plan.scope.clone(),
                    manifest_root,
                    history_digest: plan.history_digest.clone(),
                });
                self.commit_operations.push(request.operation_id);
                if self.lose_ack {
                    Answer::Fail(crate::error::unavailable(Verb::TurnCheckpoint, 503))
                } else {
                    json(&Response::Committed(Box::new(self.head.clone().unwrap())))
                }
            }
            Action::Abandon { .. } => Self::conflict(),
            _ => Answer::Fail(crate::error::config("unexpected native action")),
        }
    }

    fn begin(&mut self, begin: af_wire::lease::TurnCheckpointBegin) -> Answer {
        if begin.expected_head != self.head.as_ref().map(|head| head.checkpoint_id.clone()) {
            return Self::conflict();
        }
        self.allocated += 1;
        let deadline = clock::after(clock::now(), Duration::from_mins(15));
        let plan = TurnCheckpointPlan {
            checkpoint_id: TurnId(format!(
                "019f0000-0000-7000-8000-{:012}",
                self.allocated + 10
            )),
            generation: self.allocated,
            format_version: af_wire::lease::TURN_CHECKPOINT_FORMAT,
            scope: snapshot().scope,
            expected_head: begin.expected_head,
            page_ids: Vec::new(),
            history_digest: begin.history_digest,
            replayed: false,
            pin: CheckpointPin {
                id: PAGE.to_owned(),
                resource_version: 1,
                expires_at: deadline,
                operation_deadline: deadline,
            },
        };
        self.plan = Some(plan.clone());
        json(&Response::Begun(Box::new(plan)))
    }

    fn conflict() -> Answer {
        Answer::Fail(crate::error::refused(
            Verb::TurnCheckpoint,
            409,
            Some(af_core::error_code::RUN_CHECKPOINT_CONFLICT),
        ))
    }
}
