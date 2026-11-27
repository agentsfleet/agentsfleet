//! A task-bound run prepares by reading its inputs before its sandbox, and its
//! root acts only within the native execution ceiling.

use crate::{
    PreparedDurableTask,
    engine::{AgentEngine as _, AgentRun, Meter},
    fixture::{Canned, Script, call, ended, payload, say, unbounded},
};
use af_wire::durable_task::*;
use afr_tools::{RunVerbs, ScheduleCall, verbs::Unanswered};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

mod behavior;

const SOURCE: &str = "019f0000-0000-7000-8000-000000000001";
const PENDING: &str = "019f0000-0000-7000-8000-000000000002";
const ITEM: &str = "019f0000-0000-7000-8000-000000000003";

#[derive(Debug)]
struct Native {
    inputs: DurableTaskInputsResponse<'static>,
    answered: AtomicBool,
}

#[async_trait::async_trait]
impl RunVerbs for Native {
    async fn schedules(&self, _: ScheduleCall<'_>) -> Result<String, Unanswered> {
        Err(Unanswered::Unreachable)
    }
    async fn message(&self, _: u64, _: &str) -> Result<bool, Unanswered> {
        Err(Unanswered::Unreachable)
    }
    async fn durable_task_inputs(
        &self,
        query: DurableTaskInputsQuery,
    ) -> Result<DurableTaskInputsResponse<'static>, Unanswered> {
        assert!(
            query.starting_after.is_none(),
            "unanswered source pages must not be skipped"
        );
        Ok(self.inputs.clone())
    }
    async fn durable_task_effects(
        &self,
        _: DurableTaskEffectsQuery,
    ) -> Result<DurableTaskEffectsPage, Unanswered> {
        Ok(DurableTaskEffectsPage {
            effects: vec![],
            next_cursor: None,
        })
    }
    async fn durable_task_input_receipt(
        &self,
        input: DurableTaskInputReceiptInput,
    ) -> Result<DurableTaskInputReceiptResult, Unanswered> {
        self.answered.store(true, Ordering::Relaxed);
        Ok(DurableTaskInputReceiptResult {
            item_id: input.item_id,
            mode: input.mode,
            covered_input_sequence: None,
            replayed: false,
        })
    }
}

fn inputs() -> DurableTaskInputsResponse<'static> {
    DurableTaskInputsResponse {
        execution_deadline: Some(af_core::clock::after(
            af_core::clock::now(),
            std::time::Duration::from_mins(10),
        )),
        source_turn_id: SOURCE.to_owned(),
        current_message: None,
        current_durable_task: None,
        current_interpretation: None,
        inputs: vec![],
        accepted_inputs: vec![],
        durable_tasks: vec![],
        next_cursor: None,
    }
}

fn task() -> DurableTaskContext<'static> {
    DurableTaskContext {
        durable_task_id: SOURCE.into(),
        originating_item_id: ITEM.into(),
        target: DurableTaskTarget::ApiOnly,
        generation: 1,
        stop_generation: 0,
        objective: "Investigate the failure".into(),
        goal_digest: "a".repeat(64).into(),
        questions: vec![],
        state: DurableTaskState::Running,
        covered_input_sequence: 0,
        input_sequence: 0,
        effect_cursor: 0,
        confirmed_effect_watermark: 0,
        checkpoint_id: None,
        checkpoint_generation: None,
        expires_at: af_core::clock::after(
            af_core::clock::now(),
            std::time::Duration::from_hours(1),
        ),
        limits: DurableTaskLimits::default(),
        usage: DurableTaskUsage::default(),
    }
}

fn pending_inputs() -> DurableTaskInputsResponse<'static> {
    let mut input = inputs();
    input.current_durable_task = Some(task());
    input.next_cursor = Some("a later source page".to_owned());
    input.inputs.push(DurableTaskHumanInput {
        source_turn_id: PENDING.to_owned(),
        sequence: 1,
        message: "hello".to_owned(),
        accepted: Some(DurableTaskAcceptedInterpretation {
            source_turn_id: PENDING.to_owned(),
            plan_digest: "b".repeat(64),
            message: "hello".to_owned(),
            items: vec![DurableTaskRecoveredItem {
                binding: DurableTaskItemBinding {
                    item_id: ITEM.to_owned(),
                    durable_task_id: Some(SOURCE.to_owned()),
                    durable_task_generation: Some(1),
                    action: DurableTaskInputAction::Steer,
                    applies_here: true,
                    input_sequence: None,
                },
                accepted_text: "hello".to_owned(),
                interpretation: "Steer the investigation".to_owned(),
                response_text: None,
            }],
        }),
    });
    input
}

async fn run(
    engine: &super::Loop,
    run: &af_wire::run::RunPayload<'_>,
    native: &dyn RunVerbs,
    prepared: &PreparedDurableTask,
) -> crate::Result<crate::RunOutput> {
    engine
        .run(AgentRun {
            recovery: None,
            run,
            prepared_durable_task: Some(prepared),
            recovered_durable_task: None,
            recovered_turn: None,
            memory: afr_memory::Seed::default(),
            agent: &afr_guest_client::AgentSlot::unavailable(
                afr_guest_client::Unavailable::NoSandbox,
            ),
            mint: &af_egress::testing::CountingMint::never(),
            verbs: native,
            checkpoint: &crate::testing::Discard,
            inbox: &crate::testing::NoInbox,
            events: &|_| {},
            meter: &Meter::default(),
            stop: &tokio_util::sync::CancellationToken::new(),
        })
        .await
}

#[tokio::test]
async fn missing_or_spent_native_deadline_blocks_actions_but_retains_recovery() -> crate::Result<()>
{
    for deadline in [None, Some(af_core::clock::now())] {
        let script = Script::new([]);
        let engine = super::tests::engine(Vec::new(), &script);
        let mut this_run = payload(&[], unbounded());
        this_run.durable_task = Some(task());
        this_run.policy.model_context_tokens = 16_384;
        let mut input = pending_inputs();
        input.execution_deadline = deadline;
        let native = Native {
            inputs: input.clone(),
            answered: AtomicBool::new(false),
        };
        let prepared = PreparedDurableTask {
            inputs: input,
            execution_deadline: deadline,
        };
        let output = run(&engine, &this_run, &native, &prepared).await?;
        assert!(!native.answered.load(Ordering::Relaxed));
        assert!(
            script.sent().is_empty(),
            "a new agent clock cannot extend native custody"
        );
        assert!(output.task.is_none());
        assert!(output.recovery.is_some());
    }
    Ok(())
}

/// Verbs whose input read answers `read`, counting each read.
#[derive(Debug)]
struct Reads {
    read: Result<DurableTaskInputsResponse<'static>, Unanswered>,
    reads: AtomicUsize,
}

impl Reads {
    fn answering(read: Result<DurableTaskInputsResponse<'static>, Unanswered>) -> Self {
        Self {
            read,
            reads: AtomicUsize::new(0),
        }
    }
}

#[async_trait::async_trait]
impl RunVerbs for Reads {
    async fn schedules(&self, _: ScheduleCall<'_>) -> Result<String, Unanswered> {
        Err(Unanswered::Unreachable)
    }
    async fn message(&self, _: u64, _: &str) -> Result<bool, Unanswered> {
        Err(Unanswered::Unreachable)
    }
    async fn durable_task_inputs(
        &self,
        query: DurableTaskInputsQuery,
    ) -> Result<DurableTaskInputsResponse<'static>, Unanswered> {
        assert_eq!(query.fencing_token, 7, "the run's own fence");
        self.reads.fetch_add(1, Ordering::Relaxed);
        self.read.clone()
    }
}

/// A run bound to no task is an ordinary run: it prepares nothing and reads
/// no input, whatever unfinished tasks the fleet has.
#[tokio::test]
async fn a_run_bound_to_no_task_prepares_nothing() -> crate::Result<()> {
    let engine = super::tests::engine(Vec::new(), &Script::new([]));
    let mut ordinary = payload(&[], unbounded());
    ordinary.decision = Some(Box::new(crate::fixture::decision_assignment()));
    ordinary.unfinished_tasks = vec![DurableTaskInputCandidate {
        durable_task_id: SOURCE.to_owned(),
        title: "Investigate the failure".to_owned(),
        state: DurableTaskState::Waiting,
        generation: 1,
    }];
    let native = Reads::answering(Ok(pending_inputs()));

    let prepared = engine.prepare_durable_task(&ordinary, &native).await?;

    assert!(prepared.is_none());
    assert_eq!(native.reads.load(Ordering::Relaxed), 0);
    Ok(())
}

/// A bound run reads its inputs once, before any sandbox, and keeps the
/// native ceiling the read carried.
#[tokio::test]
async fn a_bound_run_prepares_from_its_native_inputs() -> crate::Result<()> {
    let engine = super::tests::engine(Vec::new(), &Script::new([]));
    let mut bound = payload(&[], unbounded());
    bound.durable_task = Some(task());
    let read = pending_inputs();
    let native = Reads::answering(Ok(read.clone()));

    let prepared = engine
        .prepare_durable_task(&bound, &native)
        .await?
        .ok_or_else(crate::decision::unavailable)?;

    assert_eq!(native.reads.load(Ordering::Relaxed), 1);
    assert_eq!(prepared.execution_deadline, read.execution_deadline);
    assert_eq!(
        prepared
            .inputs
            .current_durable_task
            .map(|task| task.durable_task_id.into_owned()),
        Some(SOURCE.to_owned())
    );
    Ok(())
}

/// A bound run whose read names no current task, or whose read is refused,
/// is not prepared: it never acts on a task the daemon did not name.
#[tokio::test]
async fn a_bound_run_without_a_readable_task_is_refused() {
    let engine = super::tests::engine(Vec::new(), &Script::new([]));
    let mut bound = payload(&[], unbounded());
    bound.durable_task = Some(task());
    let refused = af_core::error_code::DURABLE_TASK_NOT_FOUND;
    for (read, code) in [
        (Ok(inputs()), af_core::error_code::INTERNAL_OPERATION_FAILED),
        (Err(Unanswered::Refused(Some(refused))), refused),
    ] {
        let native = Reads::answering(read);

        let failure = engine.prepare_durable_task(&bound, &native).await.err();

        assert_eq!(failure.map(|failure| failure.code()), Some(code));
    }
}
