//! A task root saves its private history at the boundary before its next model
//! call, and only a receipt for that capture advances the task's checkpoint.

#![expect(
    clippy::expect_used,
    reason = "test fixture: poisoned state must fail the test"
)]

use super::*;
use crate::{
    CommittedRecovery, FileRecoveryPermit, RecoveryBoundarySink, RecoveryReceipt, RecoverySave,
    RecoverySkip, RecoverySnapshot, RecoveryState,
};

const CHECKPOINT: &str = "019f0000-0000-7000-8000-0000000000c1";

/// A capture owner that commits each task capture it is handed.
#[derive(Debug, Default)]
struct Committing {
    saved: Mutex<Vec<(u64, u64)>>,
}

#[async_trait::async_trait]
impl RecoveryBoundarySink for Committing {
    fn workspace(&self) -> Result<af_wire::run::TurnWorkspace, RecoverySkip> {
        Err(RecoverySkip::WorkspaceBuilding)
    }

    fn try_reserve(&self, task: bool) -> Result<Option<FileRecoveryPermit>, RecoverySkip> {
        assert!(task, "a task root reserves as a task");
        Ok(None)
    }

    async fn save(&self, snapshot: RecoverySnapshot) -> crate::Result<RecoverySave> {
        let RecoveryState::DurableTask(task) = &snapshot.state else {
            return Err(crate::error::recovery_invalid("a task root saved a turn"));
        };
        self.saved
            .lock()
            .expect("fixture mutex is unpoisoned")
            .push((
                task.task.covered_input_sequence,
                task.task.confirmed_effect_watermark,
            ));
        Ok(RecoverySave::Committed(Box::new(RecoveryReceipt {
            run_id: snapshot.run_id,
            turn_id: snapshot.turn_id,
            fencing_token: snapshot.fencing_token,
            checkpoint: CommittedRecovery::DurableTask {
                checkpoint_id: CHECKPOINT.to_owned(),
                generation: task.task.checkpoint_generation.unwrap_or(0) + 1,
                history_digest: af_core::etag::sha256_hex(&task.history),
            },
        })))
    }
}

#[tokio::test]
async fn a_committed_boundary_receipt_advances_the_task_checkpoint_it_reports() -> crate::Result<()>
{
    let script = Script::new([
        vec![say("Still investigating."), ended(false)],
        vec![say("Investigation recorded."), ended(false)],
    ]);
    let engine = super::super::super::tests::engine(vec![], &script);
    let this_run = assigned_run(&[], 16_384);
    let native = goals([
        evaluation("CONTINUE", Some(1.0), 0.0),
        evaluation("DONE", Some(1.0), 1.0),
    ]);
    let prepared = PreparedDurableTask {
        execution_deadline: native.inputs.execution_deadline,
        inputs: native.inputs.clone(),
    };
    let sink = Committing::default();
    let output = engine
        .run(AgentRun {
            recovery: Some(&sink),
            run: &this_run,
            prepared_durable_task: Some(&prepared),
            recovered_durable_task: None,
            recovered_turn: None,
            memory: afr_memory::Seed::default(),
            agent: &afr_guest_client::AgentSlot::unavailable(
                afr_guest_client::Unavailable::NoSandbox,
            ),
            mint: &af_egress::testing::CountingMint::never(),
            verbs: &native,
            checkpoint: &crate::testing::Discard,
            inbox: &crate::testing::NoInbox,
            events: &|_| {},
            meter: &Meter::default(),
            stop: &tokio_util::sync::CancellationToken::new(),
        })
        .await?;

    assert_eq!(
        *sink.saved.lock().expect("fixture mutex is unpoisoned"),
        vec![(0, 0)],
        "one boundary before the second model call"
    );
    let report = output.task.ok_or_else(crate::decision::unavailable)?;
    assert_eq!(report.disposition, DurableTaskDisposition::Complete);
    let recovery = output.recovery.ok_or_else(crate::decision::unavailable)?;
    assert_eq!(recovery.task.checkpoint_id.as_deref(), Some(CHECKPOINT));
    assert_eq!(recovery.task.checkpoint_generation, Some(1));
    Ok(())
}

#[tokio::test]
async fn recovered_history_without_a_current_native_task_is_refused_before_any_model_call()
-> crate::Result<()> {
    let script = Script::new([vec![say("Never asked."), ended(false)]]);
    let engine = super::super::super::tests::engine(vec![], &script);
    let this_run = assigned_run(&[], 16_384);
    let native = goals([]);
    let mut inputs = native.inputs.clone();
    inputs.current_durable_task = None;
    let prepared = PreparedDurableTask {
        execution_deadline: inputs.execution_deadline,
        inputs,
    };
    let recovered = crate::DurableTaskRecovery {
        task: task(),
        history: zeroize::Zeroizing::new(b"{}".to_vec()),
    };

    let refused = engine
        .run(AgentRun {
            recovery: None,
            run: &this_run,
            prepared_durable_task: Some(&prepared),
            recovered_durable_task: Some(&recovered),
            recovered_turn: None,
            memory: afr_memory::Seed::default(),
            agent: &afr_guest_client::AgentSlot::unavailable(
                afr_guest_client::Unavailable::NoSandbox,
            ),
            mint: &af_egress::testing::CountingMint::never(),
            verbs: &native,
            checkpoint: &crate::testing::Discard,
            inbox: &crate::testing::NoInbox,
            events: &|_| {},
            meter: &Meter::default(),
            stop: &tokio_util::sync::CancellationToken::new(),
        })
        .await
        .err()
        .ok_or_else(crate::decision::unavailable)?;

    assert!(
        refused
            .to_string()
            .contains("recovered history has no current native task"),
        "{refused}"
    );
    assert!(script.sent().is_empty(), "the model was called");
    Ok(())
}
