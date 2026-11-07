//! Schema-valid external native decisions and task descriptions for kernel callers.
use super::{Config, TASK};
use afd_wire::{
    decision::{
        ChoiceOption, DecisionAnswer, DecisionAssignment, DecisionOutcome, DecisionPurpose,
        DecisionQuestion, DecisionRequest, DecisionResult, DecisionUsage, NamedQuestion,
        answer_matches,
    },
    task::{
        DecisionAssessment, DecisionReceipt, DecisionReceiptInput, DecisionRecordDisposition,
        DecisionRecorded, DecisionReservationInput, TaskContext, TaskInputsResponse, TaskLimits,
        TaskState, TaskUsage,
    },
};
use garde::Validate as _;
use libtest_mimic::Failed;
use serde_json::{Value, json};
pub(super) fn deadline() -> jiff::Timestamp {
    afd_core::clock::after(afd_core::clock::now(), std::time::Duration::from_mins(5))
}
pub(super) fn assignment() -> Result<DecisionAssignment, Failed> {
    let threshold = json!({"probability_bps":9000,"confidence_bps":9000});
    let assignment: DecisionAssignment = serde_json::from_value(json!({"revision_id":TASK,
        "profile":{"model_id":TASK,"revision":"reviewed","api":"typesafe","endpoint":"https://decision.fixture.test/v1","model":"decision-model","accepted_models":["decision-model"],"capabilities":{"noul":true,"choice":true,"score":false},"input_max_bytes":24576},
        "intent":{"questions_revision":"dt-routing-v1","intent":threshold,"association":threshold,"single_request_max_bps":1000,"multiple_request_min_bps":9000},
        "goal":{"questions_revision":"dt-goal-v1","statement_min_bps":9000,"continue_action":threshold,"wait":threshold,"done":threshold}}))?;
    assignment.validate()?;
    Ok(assignment)
}
pub(super) fn inputs(config: &Config) -> Result<Value, Failed> {
    let task = TaskContext {
        task_id: TASK.into(),
        originating_item_id: TASK.into(),
        target: config.target(),
        generation: 1,
        stop_generation: 0,
        accepted_revision_id: TASK.into(),
        objective: "Capture accepted progress".into(),
        goal_digest: "a".repeat(64).into(),
        questions: vec![],
        state: TaskState::Running,
        covered_input_sequence: 0,
        input_sequence: 0,
        effect_cursor: 0,
        confirmed_effect_watermark: 0,
        checkpoint_id: None,
        checkpoint_generation: None,
        expires_at: deadline(),
        limits: TaskLimits::default(),
        usage: TaskUsage::default(),
    };
    Ok(serde_json::to_value(TaskInputsResponse {
        source_admission_id: TASK.to_owned(),
        accepted_revision_id: Some(TASK.to_owned()),
        current_message: None,
        inputs: vec![],
        accepted_inputs: vec![],
        current_task: Some(task),
        execution_deadline: Some(deadline()),
        current_interpretation: None,
        tasks: vec![],
        next_cursor: None,
    })?)
}
pub(super) fn decision(body: Value) -> Result<Value, Failed> {
    let input: DecisionReservationInput = serde_json::from_value(body)?;
    input.validate()?;
    reservation(&input)?;
    let (question, answer) = wait_answer()?;
    let request = DecisionRequest {
        call_id: input.operation_id.clone(),
        purpose: DecisionPurpose::Goal,
        state: input.evidence,
        questions: vec![question],
    };
    request.validate()?;
    let result = DecisionResult {
        reported_model: Some("decision-model".to_owned()),
        usage: Some(DecisionUsage {
            input_tokens: 10,
            output_tokens: 2,
            cached_input_tokens: 0,
            cache_write_tokens: 0,
        }),
        result: DecisionOutcome::Answered {
            answers: std::collections::BTreeMap::from([("next_action".to_owned(), answer)]),
        },
    };
    DecisionReceiptInput {
        fencing_token: input.fencing_token,
        receipt: DecisionReceipt::Provider {
            result: result.clone(),
        },
    }
    .validate()?;
    Ok(
        json!({"status":"existing","call_id":input.operation_id,"state":"known","recorded":DecisionRecorded { request,result,disposition:DecisionRecordDisposition::Current }}),
    )
}

fn wait_answer() -> Result<(NamedQuestion, DecisionAnswer), Failed> {
    let question = NamedQuestion {
        name: "next_action".to_owned(),
        question: DecisionQuestion::Choice {
            instructions: "Choose the next action supported by completed work.".to_owned(),
            options: ["DONE", "WAIT", "CONTINUE"]
                .into_iter()
                .map(|value| ChoiceOption {
                    value: value.to_owned(),
                    description: format!("Evidence supports {value}"),
                })
                .collect(),
        },
    };
    let answer = DecisionAnswer::Choice {
        choice: "WAIT".to_owned(),
        confidence: Some(1.0),
        probabilities: std::collections::BTreeMap::from([
            ("DONE".to_owned(), 0.0),
            ("WAIT".to_owned(), 1.0),
            ("CONTINUE".to_owned(), 0.0),
        ]),
    };
    crate::run::expect(
        answer_matches(&question.question, &answer),
        "typed complete probability distribution",
    )?;
    Ok((question, answer))
}

fn reservation(input: &DecisionReservationInput) -> Result<(), Failed> {
    crate::run::expect(
        matches!(input.assessment, DecisionAssessment::Goal { .. }),
        "one native goal decision",
    )?;
    crate::run::expect(
        input.source_admission_id == TASK
            && input
                .task
                .as_ref()
                .is_some_and(|task| task.task_id == TASK && task.generation == 1),
        "accepted goal task and source",
    )
}
