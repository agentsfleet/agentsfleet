//! Initial routing binds complete meaning once and recovers its native acknowledgement.

#![expect(
    clippy::expect_used,
    reason = "test fixture: poisoned state must fail the test"
)]

use super::*;

#[derive(Debug)]
pub(super) struct IntakeNative {
    pub(super) inputs: Mutex<TaskInputsResponse<'static>>,
    pub(super) replies: Mutex<VecDeque<DecisionEvaluation>>,
    pub(super) decisions: Mutex<Vec<DecisionReservationInput>>,
    pub(super) bindings: Mutex<Vec<TaskBindingsInput>>,
    pub(super) lose_acknowledgement: bool,
}

#[async_trait::async_trait]
impl LeaseVerbs for IntakeNative {
    async fn schedules(&self, _: ScheduleCall<'_>) -> Result<String, Unanswered> {
        Err(Unanswered::Unreachable)
    }
    async fn message(&self, _: &str) -> Result<bool, Unanswered> {
        Err(Unanswered::Unreachable)
    }
    async fn task_inputs(
        &self,
        _: TaskInputsQuery,
    ) -> Result<TaskInputsResponse<'static>, Unanswered> {
        Ok(self
            .inputs
            .lock()
            .expect("fixture mutex is unpoisoned")
            .clone())
    }
    async fn task_decision(
        &self,
        request: DecisionReservationInput,
    ) -> Result<DecisionEvaluation, Unanswered> {
        let call_id = request.operation_id.clone();
        self.decisions
            .lock()
            .expect("fixture mutex is unpoisoned")
            .push(request);
        let mut evaluation = self
            .replies
            .lock()
            .expect("fixture mutex is unpoisoned")
            .pop_front()
            .ok_or(Unanswered::Unreachable)?;
        evaluation.call_id.clone_from(&call_id);
        if let Some(recorded) = evaluation.recorded.as_mut() {
            recorded.request.call_id = call_id;
        }
        Ok(evaluation)
    }
    async fn task_bindings(
        &self,
        request: TaskBindingsInput,
    ) -> Result<TaskBindingsResponse<'static>, Unanswered> {
        let mut inputs = self.inputs.lock().expect("fixture mutex is unpoisoned");
        let items: Vec<_> = request
            .items
            .iter()
            .enumerate()
            .map(|(ordinal, item)| TaskRecoveredItem {
                binding: TaskItemBinding {
                    item_id: if ordinal == 0 { ITEM } else { PENDING }.to_owned(),
                    task_id: item.proposed.task_id.clone(),
                    task_generation: item.proposed.task_id.as_ref().map(|_| 1),
                    action: item.proposed.action,
                    applies_here: item.proposed.task_id.is_some(),
                    input_sequence: item.proposed.task_id.as_ref().map(|_| 1),
                },
                accepted_text: inputs
                    .current_message
                    .as_deref()
                    .expect("fixture has a source message")
                    .get(
                        usize::try_from(item.proposed.source_start).expect("validated start")
                            ..usize::try_from(item.proposed.source_end).expect("validated end"),
                    )
                    .expect("validated source span")
                    .to_owned(),
                interpretation: item.proposed.interpretation.clone(),
                response_text: None,
            })
            .collect();
        let bindings = items.iter().map(|item| item.binding.clone()).collect();
        inputs.current_interpretation = Some(TaskAcceptedInterpretation {
            source_admission_id: request.source_admission_id.clone(),
            plan_digest: request.plan_digest.clone(),
            message: inputs.current_message.clone().unwrap_or_default(),
            items,
        });
        self.bindings
            .lock()
            .expect("fixture mutex is unpoisoned")
            .push(request);
        if self.lose_acknowledgement {
            return Err(Unanswered::Unreachable);
        }
        Ok(TaskBindingsResponse {
            items: bindings,
            current_task: inputs.current_task.clone(),
            execution_deadline: inputs.execution_deadline,
            replayed: false,
        })
    }
}

pub(super) fn route(action: &str, association: &str, multiple: f64) -> DecisionEvaluation {
    let mut result = evaluation("CONTINUE", Some(1.0), 0.0);
    let intents = [
        "conversation",
        "status",
        "refresh",
        "steer",
        "new_work",
        "clarify",
    ];
    let associations = ["task_1", "new_work", "unclear"];
    if let Some(recorded) = result.recorded.as_mut() {
        recorded.request.purpose = DecisionPurpose::Intent;
        recorded.request.state =
            serde_json::json!({"current_work":[{"alias":"task_1","task_id":SOURCE}]});
        let mut multiplicity = question();
        multiplicity.name = "multiple_requests".to_owned();
        recorded.request.questions = vec![
            multiplicity,
            choice_question("intent", &intents),
            choice_question("association", &associations),
        ];
        recorded.result.result = DecisionOutcome::Answered {
            answers: BTreeMap::from([
                (
                    "multiple_requests".to_owned(),
                    DecisionAnswer::Noul { noul: multiple },
                ),
                (
                    "intent".to_owned(),
                    choice_answer(action, Some(1.0), &intents),
                ),
                (
                    "association".to_owned(),
                    choice_answer(association, Some(1.0), &associations),
                ),
            ]),
        };
    }
    result
}

pub(super) fn native(
    message: &str,
    replies: impl IntoIterator<Item = DecisionEvaluation>,
) -> IntakeNative {
    let mut input = inputs();
    input.current_message = Some(message.to_owned());
    input.current_task = Some(task());
    IntakeNative {
        inputs: Mutex::new(input),
        replies: Mutex::new(replies.into_iter().collect()),
        decisions: Mutex::new(vec![]),
        bindings: Mutex::new(vec![]),
        lose_acknowledgement: false,
    }
}

pub(super) fn proposed(
    action: &str,
    task_id: Option<&str>,
    start: i32,
    end: i32,
) -> serde_json::Value {
    serde_json::json!({"source_start":start,"source_end":end,"action":action,"task_id":task_id,
        "title":"Handle source request","interpretation":"Preserve the complete request","target":null})
}

pub(super) async fn prepare(script: &Script, native: &IntakeNative) -> crate::Result<PreparedTask> {
    let engine = super::super::super::tests::engine(vec![], script);
    let lease = assigned_lease(&[], 16_384);
    engine
        .prepare_task(
            &lease,
            native,
            &Meter::default(),
            &tokio_util::sync::CancellationToken::new(),
        )
        .await?
        .ok_or_else(crate::decision::unavailable)
}
