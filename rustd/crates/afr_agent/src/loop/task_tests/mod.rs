//! Root execution preserves source replies and refuses uncertain initial intent.

use crate::{
    PreparedTask,
    engine::{AgentEngine as _, AgentRun, Meter},
    fixture::{Canned, Script, call, ended, lease, say, unbounded},
};
use afd_wire::task::*;
use afr_tools::{LeaseVerbs, ScheduleCall, verbs::Unanswered};
use std::sync::atomic::{AtomicBool, Ordering};

mod behavior;

const SOURCE: &str = "019f0000-0000-7000-8000-000000000001";
const PENDING: &str = "019f0000-0000-7000-8000-000000000002";
const ITEM: &str = "019f0000-0000-7000-8000-000000000003";

#[derive(Debug)]
struct Native {
    inputs: TaskInputsResponse<'static>,
    answered: AtomicBool,
}

#[async_trait::async_trait]
impl LeaseVerbs for Native {
    async fn schedules(&self, _: ScheduleCall<'_>) -> Result<String, Unanswered> {
        Err(Unanswered::Unreachable)
    }
    async fn message(&self, _: &str) -> Result<bool, Unanswered> {
        Err(Unanswered::Unreachable)
    }
    async fn task_inputs(
        &self,
        query: TaskInputsQuery,
    ) -> Result<TaskInputsResponse<'static>, Unanswered> {
        assert!(
            query.starting_after.is_none(),
            "unanswered source pages must not be skipped"
        );
        let mut inputs = self.inputs.clone();
        if self.answered.load(Ordering::Relaxed) {
            inputs.inputs.clear();
        }
        Ok(inputs)
    }
    async fn task_effects(&self, _: TaskEffectsQuery) -> Result<TaskEffectsPage, Unanswered> {
        Ok(TaskEffectsPage {
            effects: vec![],
            next_cursor: None,
        })
    }
    async fn task_input_receipt(
        &self,
        input: TaskInputReceiptInput,
    ) -> Result<TaskInputReceiptResult, Unanswered> {
        assert_eq!(input.source_admission_id, PENDING);
        assert_eq!(input.item_id, ITEM);
        assert_eq!(input.mode, TaskInputMode::Answered);
        assert_eq!(
            input.response_text.as_deref(),
            Some("Hello; your repair is still running.")
        );
        assert!(
            !self.answered.swap(true, Ordering::Relaxed),
            "the original item is answered only once"
        );
        Ok(TaskInputReceiptResult {
            item_id: input.item_id,
            mode: input.mode,
            covered_input_sequence: None,
            replayed: false,
        })
    }
}

fn inputs() -> TaskInputsResponse<'static> {
    TaskInputsResponse {
        execution_deadline: Some(afd_core::clock::after(
            afd_core::clock::now(),
            std::time::Duration::from_mins(10),
        )),
        source_admission_id: SOURCE.to_owned(),
        accepted_revision_id: Some(SOURCE.to_owned()),
        current_message: None,
        current_task: None,
        current_interpretation: None,
        inputs: vec![],
        accepted_inputs: vec![],
        tasks: vec![],
        next_cursor: None,
    }
}

fn task() -> TaskContext<'static> {
    TaskContext {
        task_id: SOURCE.into(),
        originating_item_id: ITEM.into(),
        target: TaskTarget::ApiOnly,
        generation: 1,
        stop_generation: 0,
        accepted_revision_id: SOURCE.into(),
        objective: "Investigate the failure".into(),
        goal_digest: "a".repeat(64).into(),
        questions: vec![],
        state: TaskState::Running,
        covered_input_sequence: 0,
        input_sequence: 0,
        effect_cursor: 0,
        confirmed_effect_watermark: 0,
        checkpoint_id: None,
        checkpoint_generation: None,
        expires_at: afd_core::clock::after(
            afd_core::clock::now(),
            std::time::Duration::from_hours(1),
        ),
        limits: TaskLimits::default(),
        usage: TaskUsage::default(),
    }
}

fn pending_inputs() -> TaskInputsResponse<'static> {
    let mut input = inputs();
    input.current_task = Some(task());
    input.next_cursor = Some("a later source page".to_owned());
    input.inputs.push(TaskHumanInput {
        source_admission_id: PENDING.to_owned(),
        sequence: 1,
        message: "hello".to_owned(),
        accepted: Some(TaskAcceptedInterpretation {
            source_admission_id: PENDING.to_owned(),
            plan_digest: "b".repeat(64),
            message: "hello".to_owned(),
            items: vec![TaskRecoveredItem {
                binding: TaskItemBinding {
                    item_id: ITEM.to_owned(),
                    task_id: None,
                    task_generation: None,
                    action: TaskInputAction::Conversation,
                    applies_here: false,
                    input_sequence: None,
                },
                accepted_text: "hello".to_owned(),
                interpretation: "Greet the person".to_owned(),
                response_text: None,
            }],
        }),
    });
    input
}

async fn run(
    engine: &super::Loop,
    lease: &afd_wire::lease::LeasePayload<'_>,
    native: &dyn LeaseVerbs,
    prepared: &PreparedTask,
) -> crate::Result<crate::RunOutput> {
    engine
        .run(AgentRun {
            lease,
            prepared_task: Some(prepared),
            recovered_task: None,
            memory: afr_memory::Seed::default(),
            executor: &afr_executor::ExecutorSlot::unavailable(
                afr_executor::Unavailable::NoSandbox,
            ),
            mint: &afr_egress::testing::CountingMint::never(),
            verbs: native,
            checkpoint: &crate::testing::Discard,
            context_checkpoints: &crate::testing::ContextMemory::default(),
            events: &|_| {},
            meter: &Meter::default(),
            stop: &tokio_util::sync::CancellationToken::new(),
        })
        .await
}

#[tokio::test]
async fn unavailable_initial_intent_returns_clarification_without_a_tool_or_coding_request()
-> crate::Result<()> {
    let script = Script::new([]);
    let engine = super::tests::engine(Vec::new(), &script);
    let mut lease = lease(&[], unbounded());
    lease.durable_tasks = true;
    let mut input = inputs();
    input.current_message = Some("Please investigate".to_owned());
    let native = Native {
        inputs: input,
        answered: AtomicBool::new(false),
    };
    let prepared = engine
        .prepare_task(
            &lease,
            &native,
            &Meter::default(),
            &tokio_util::sync::CancellationToken::new(),
        )
        .await?
        .ok_or_else(crate::decision::unavailable)?;
    assert!(!prepared.allow_actions);
    let output = run(&engine, &lease, &native, &prepared).await?;
    assert!(output.result.content.contains("Please clarify"));
    assert!(output.task.is_none());
    assert!(output.recovery.is_none());
    assert!(script.sent().is_empty());
    Ok(())
}

#[tokio::test]
async fn pending_conversation_reply_precedes_actions_even_when_a_later_source_page_exists()
-> crate::Result<()> {
    let tool = &afr_tools::catalog::UPDATE_PLAN;
    let script = Script::new([
        vec![say("Hello; your repair is still running."), ended(false)],
        vec![
            call("plan", tool.name(), serde_json::json!({})),
            ended(false),
        ],
    ]);
    let engine = super::tests::engine(vec![Canned::boxed(tool, "Plan updated")], &script);
    let mut lease = lease(&[tool.name()], unbounded());
    lease.durable_tasks = true;
    // A resolved physical model context is required for the original-source reply.
    // The fixture's unlimited operating cap does not resolve this provider bound.
    lease.policy.model_context_tokens = 16_384;
    let input = pending_inputs();
    let native = Native {
        inputs: input.clone(),
        answered: AtomicBool::new(false),
    };
    let prepared = PreparedTask {
        execution_deadline: Some(afd_core::clock::after(
            afd_core::clock::now(),
            std::time::Duration::from_mins(10),
        )),
        inputs: input,
        allow_actions: true,
    };
    let output = run(&engine, &lease, &native, &prepared).await?;
    assert!(native.answered.load(Ordering::Relaxed));
    let sent = script.sent();
    assert_eq!(sent.len(), 2);
    let [reply, action] = sent.as_slice() else {
        return Err(crate::decision::unavailable());
    };
    assert_eq!(reply.tools, Vec::<String>::new());
    assert!(action.tools.iter().any(|name| name == tool.name()));
    assert!(
        output.task.is_none(),
        "no decision assignment can authorize a goal result"
    );
    assert!(
        output.recovery.is_some(),
        "an unavailable judgment still retains complete private work"
    );
    Ok(())
}

#[tokio::test]
async fn unresolved_model_context_leaves_source_unanswered_and_blocks_actions() -> crate::Result<()>
{
    let tool = &afr_tools::catalog::UPDATE_PLAN;
    let script = Script::new([]);
    let engine = super::tests::engine(vec![Canned::boxed(tool, "Plan updated")], &script);
    let mut lease = lease(&[tool.name()], unbounded());
    lease.durable_tasks = true;
    lease.policy.model_context_tokens = 0;
    let input = pending_inputs();
    let native = Native {
        inputs: input.clone(),
        answered: AtomicBool::new(false),
    };
    let prepared = PreparedTask {
        inputs: input,
        allow_actions: true,
        execution_deadline: Some(afd_core::clock::after(
            afd_core::clock::now(),
            std::time::Duration::from_mins(10),
        )),
    };
    let output = run(&engine, &lease, &native, &prepared).await?;
    assert!(!native.answered.load(Ordering::Relaxed));
    assert!(
        script.sent().is_empty(),
        "unresolved context permits neither a reply nor an action request"
    );
    assert!(
        output.task.is_none(),
        "an unanswered source cannot authorize task completion"
    );
    assert!(
        output.recovery.is_some(),
        "pending obligations remain recoverable"
    );
    Ok(())
}

#[tokio::test]
async fn missing_or_spent_native_deadline_blocks_actions_but_retains_recovery() -> crate::Result<()>
{
    for deadline in [None, Some(afd_core::clock::now())] {
        let script = Script::new([]);
        let engine = super::tests::engine(Vec::new(), &script);
        let mut lease = lease(&[], unbounded());
        lease.durable_tasks = true;
        lease.policy.model_context_tokens = 16_384;
        let mut input = pending_inputs();
        input.execution_deadline = deadline;
        let native = Native {
            inputs: input.clone(),
            answered: AtomicBool::new(false),
        };
        let prepared = PreparedTask {
            inputs: input,
            allow_actions: true,
            execution_deadline: deadline,
        };
        let output = run(&engine, &lease, &native, &prepared).await?;
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
