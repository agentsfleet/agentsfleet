//! A message no ordinary run bound with `task_input` never reaches a task run:
//! the task run neither binds it nor answers it, and its model never reads it.
use super::*;

/// The message a person sent while the task ran, which no run bound.
const UNBOUND: &str = "is it done yet?";

#[derive(Debug)]
struct Queued {
    goals: Goals,
    bindings: AtomicUsize,
    receipts: AtomicUsize,
}

#[async_trait::async_trait]
impl RunVerbs for Queued {
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
        self.goals.durable_task_inputs(query).await
    }
    async fn durable_task_bindings(
        &self,
        _: DurableTaskBindingsInput,
    ) -> Result<DurableTaskBindingsResponse<'static>, Unanswered> {
        self.bindings.fetch_add(1, Ordering::Relaxed);
        Err(Unanswered::Unreachable)
    }
    async fn durable_task_decision(
        &self,
        input: DecisionReservationInput,
    ) -> Result<DecisionEvaluation, Unanswered> {
        self.goals.durable_task_decision(input).await
    }
    async fn durable_task_effects(
        &self,
        query: DurableTaskEffectsQuery,
    ) -> Result<DurableTaskEffectsPage, Unanswered> {
        self.goals.durable_task_effects(query).await
    }
    async fn durable_task_input_receipt(
        &self,
        _: DurableTaskInputReceiptInput,
    ) -> Result<DurableTaskInputReceiptResult, Unanswered> {
        self.receipts.fetch_add(1, Ordering::Relaxed);
        Err(Unanswered::Unreachable)
    }
}

#[tokio::test]
async fn a_message_no_run_bound_never_reaches_the_task_run() -> crate::Result<()> {
    let mut goals = goals([evaluation("WAIT", Some(1.0), 0.0)]);
    goals.inputs.inputs.push(DurableTaskHumanInput {
        source_turn_id: PENDING.to_owned(),
        sequence: 1,
        message: UNBOUND.to_owned(),
        accepted: None,
    });
    let native = Queued {
        goals,
        bindings: AtomicUsize::new(0),
        receipts: AtomicUsize::new(0),
    };
    let tool = &afr_tools::catalog::UPDATE_PLAN;
    let script = Script::new([vec![
        call("plan", tool.name(), serde_json::json!({})),
        ended(false),
    ]]);
    let engine =
        super::super::super::tests::engine(vec![Canned::boxed(tool, "Plan updated")], &script);
    let this_run = assigned_run(&[tool.name()], 16_384);
    let prepared = PreparedDurableTask {
        execution_deadline: native.goals.inputs.execution_deadline,
        inputs: native.goals.inputs.clone(),
    };

    let output = run(&engine, &this_run, &native, &prepared).await?;

    assert_eq!(
        output
            .task
            .ok_or_else(crate::decision::unavailable)?
            .disposition,
        DurableTaskDisposition::Waiting
    );
    assert_eq!(native.bindings.load(Ordering::Relaxed), 0, "no binding");
    assert_eq!(native.receipts.load(Ordering::Relaxed), 0, "no answer");
    let sent = format!("{:?}", script.sent());
    assert!(!sent.contains(UNBOUND), "the model never read it: {sent}");
    Ok(())
}
