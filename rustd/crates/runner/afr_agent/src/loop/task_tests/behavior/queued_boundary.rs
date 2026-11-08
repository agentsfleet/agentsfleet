//! Queued source meaning is bound and reread before another root action is allowed.
use super::*;

#[derive(Debug)]
struct Queued {
    intake: intake_fixture::IntakeNative,
    goals: Goals,
    queries: Mutex<Vec<TaskInputsQuery>>,
    receipts: Mutex<Vec<TaskInputReceiptInput>>,
}

#[async_trait::async_trait]
impl LeaseVerbs for Queued {
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
        self.queries
            .lock()
            .expect("input reads")
            .push(query.clone());
        self.intake.task_inputs(query).await
    }
    async fn task_bindings(
        &self,
        request: TaskBindingsInput,
    ) -> Result<TaskBindingsResponse<'static>, Unanswered> {
        let result = self.intake.task_bindings(request).await?;
        self.intake
            .inputs
            .lock()
            .expect("accepted input")
            .inputs
            .clear();
        Ok(result)
    }
    async fn task_decision(
        &self,
        input: DecisionReservationInput,
    ) -> Result<DecisionEvaluation, Unanswered> {
        if matches!(input.assessment, DecisionAssessment::Goal { .. }) {
            self.goals.task_decision(input).await
        } else {
            self.intake.task_decision(input).await
        }
    }
    async fn task_effects(&self, query: TaskEffectsQuery) -> Result<TaskEffectsPage, Unanswered> {
        self.goals.task_effects(query).await
    }
    async fn task_input_receipt(
        &self,
        input: TaskInputReceiptInput,
    ) -> Result<TaskInputReceiptResult, Unanswered> {
        assert_eq!(input.mode, TaskInputMode::Answered);
        assert_eq!(input.source_admission_id, SOURCE);
        let result = TaskInputReceiptResult {
            item_id: input.item_id.clone(),
            mode: input.mode,
            covered_input_sequence: None,
            replayed: false,
        };
        if let Some(source) = self
            .intake
            .inputs
            .lock()
            .expect("accepted input")
            .current_interpretation
            .as_mut()
        {
            for item in &mut source.items {
                if item.binding.item_id == input.item_id {
                    item.response_text.clone_from(&input.response_text);
                }
            }
        }
        self.receipts.lock().expect("input receipts").push(input);
        Ok(result)
    }
}

fn queued() -> Queued {
    let intake = intake_fixture::native(
        "hello",
        [intake_fixture::route("conversation", "unclear", 0.0)],
    );
    {
        let mut inputs = intake.inputs.lock().expect("queued source");
        inputs.inputs.push(TaskHumanInput {
            source_admission_id: SOURCE.to_owned(),
            sequence: 1,
            message: "hello".to_owned(),
            accepted: None,
        });
        if let Some(task) = inputs.current_task.as_mut() {
            task.questions = vec![question()];
        }
    }
    Queued {
        intake,
        goals: goals([evaluation("WAIT", Some(1.0), 0.0)]),
        queries: Mutex::new(vec![]),
        receipts: Mutex::new(vec![]),
    }
}

#[tokio::test]
async fn queued_message_is_bound_once_then_reread_and_answered_before_root_work()
-> crate::Result<()> {
    let native = queued();
    let plan =
        serde_json::json!([intake_fixture::proposed("conversation", None, 0, 5)]).to_string();
    let script = Script::new([
        vec![say(&plan), ended(false)],
        vec![say("Hello."), ended(false)],
        vec![say("Investigation awaits checks."), ended(false)],
    ]);
    let engine = super::super::super::tests::engine(vec![], &script);
    let lease = assigned_lease(&[], 16_384);
    let inputs = native
        .intake
        .inputs
        .lock()
        .expect("prepared inputs")
        .clone();
    let prepared = PreparedTask {
        execution_deadline: inputs.execution_deadline,
        inputs,
        allow_actions: true,
    };
    let output = run(&engine, &lease, &native, &prepared).await?;
    assert_queued_outcome(&native, &script, output)
}

fn assert_queued_outcome(
    native: &Queued,
    script: &Script,
    output: crate::RunOutput,
) -> crate::Result<()> {
    assert_eq!(
        output
            .task
            .ok_or_else(crate::decision::unavailable)?
            .covered_input_sequence,
        0
    );
    assert!(output.result.content.contains("Hello."));
    assert!(
        output
            .result
            .content
            .contains("Investigation awaits checks.")
    );
    assert_eq!(script.sent().len(), 3);
    assert_eq!(native.intake.bindings.lock().expect("bound plans").len(), 1);
    assert_eq!(
        native.intake.decisions.lock().expect("intent calls").len(),
        1
    );
    let receipts = native.receipts.lock().expect("source receipts");
    assert_eq!(receipts.len(), 1);
    assert_eq!(
        receipts
            .first()
            .and_then(|receipt| receipt.response_text.as_deref()),
        Some("Hello.")
    );
    let reads = native.queries.lock().expect("input reads");
    assert!(reads.len() >= 2);
    assert!(
        reads
            .iter()
            .all(|query| query.after_input_sequence == Some(0) && query.starting_after.is_none())
    );
    Ok(())
}
