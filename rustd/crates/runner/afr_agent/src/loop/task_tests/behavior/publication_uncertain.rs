//! An uncertain daemon publication reply is exposed as failed until reconciled.
use super::publication_fixture::{Publisher, candidate, publisher};
use super::*;
use afr_tools::sandbox::{ScriptedExecutor, ScriptedProcess};

#[derive(Debug)]
struct Uncertain {
    native: Publisher,
    prior: bool,
}

#[async_trait::async_trait]
impl LeaseVerbs for Uncertain {
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
        self.native.task_inputs(query).await
    }
    async fn task_decision(
        &self,
        input: DecisionReservationInput,
    ) -> Result<DecisionEvaluation, Unanswered> {
        self.native.task_decision(input).await
    }
    async fn task_effects(&self, query: TaskEffectsQuery) -> Result<TaskEffectsPage, Unanswered> {
        if self.prior && query.limit == 1 {
            let mut effect = Publisher::receipt();
            effect.state = TaskEffectState::Unknown;
            effect.commit = None;
            return Ok(TaskEffectsPage {
                effects: vec![effect],
                next_cursor: None,
            });
        }
        self.native.task_effects(query).await
    }
    async fn task_publish(&self, input: TaskEffectInput) -> Result<TaskEffect, Unanswered> {
        let mut receipt = self.native.task_publish(input).await?;
        receipt.state = TaskEffectState::Unknown;
        receipt.commit = None;
        Ok(receipt)
    }
}

#[tokio::test]
async fn prior_unreconciled_effect_prevents_capture_and_new_publication() -> crate::Result<()> {
    let native = Uncertain {
        native: publisher(),
        prior: true,
    };
    let script = Script::new([vec![
        super::publication_guards::publication_call(),
        ended(false),
    ]]);
    let executor = ScriptedExecutor::new([]);
    let output = super::publication_guards::execute(
        &script,
        &native,
        native.native.goals.inputs.clone(),
        &executor,
    )
    .await?;
    assert_eq!(executor.spawned().len(), 0);
    super::publication_guards::refusal(
        &native.native,
        output,
        "previous publication must be reconciled",
    )?;
    Ok(())
}

#[tokio::test]
async fn uncertain_publication_reply_does_not_claim_confirmed_tool_success() -> crate::Result<()> {
    let native = Uncertain {
        native: publisher(),
        prior: false,
    };
    let script = Script::new([vec![
        super::publication_guards::publication_call(),
        ended(false),
    ]]);
    let executor = ScriptedExecutor::new([
        ScriptedProcess::exits(&candidate(), 0),
        ScriptedProcess::exits("checks passed", 0),
        ScriptedProcess::exits(&candidate(), 0),
    ]);
    let output = super::publication_guards::execute(
        &script,
        &native,
        native.native.goals.inputs.clone(),
        &executor,
    )
    .await?;
    assert_uncertain_outcome(&native, &executor, output)
}

fn assert_uncertain_outcome(
    native: &Uncertain,
    executor: &ScriptedExecutor,
    output: crate::RunOutput,
) -> crate::Result<()> {
    let trace = output
        .trace
        .as_ref()
        .ok_or_else(crate::decision::unavailable)?;
    assert_eq!(trace.calls.len(), 1);
    let call = trace
        .calls
        .first()
        .ok_or_else(crate::decision::unavailable)?;
    assert_eq!(call.name, "propose_change");
    assert_eq!(call.status, afd_wire::tool_trace::ToolCallStatus::Failed);
    let record = output
        .records
        .first()
        .ok_or_else(crate::decision::unavailable)?;
    assert_eq!(
        record.output.split_once(' ').map(|(code, _)| code),
        Some("[agentsfleetd_refused]")
    );
    assert_eq!(executor.spawned().len(), 3);
    assert_eq!(
        native
            .native
            .published
            .lock()
            .expect("publication attempts")
            .len(),
        1
    );
    assert_eq!(
        output
            .task
            .ok_or_else(crate::decision::unavailable)?
            .effect_watermark,
        1
    );
    assert_reconciled_evidence(native)
}

fn assert_reconciled_evidence(native: &Uncertain) -> crate::Result<()> {
    let requests = native.native.goals.requests.lock().expect("goal requests");
    let evidence = requests
        .first()
        .ok_or_else(crate::decision::unavailable)?
        .evidence
        .to_string();
    assert!(evidence.contains("unknown"));
    assert!(evidence.contains("native_publication_receipt"));
    Ok(())
}
