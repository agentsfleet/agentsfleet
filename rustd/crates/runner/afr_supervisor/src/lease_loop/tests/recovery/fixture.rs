//! Native HTTP fixture identities, with an API-only task and known goal receipt.
use super::{CHECKPOINT, ID, PAGE};
use afd_wire::{decision::*, task::*};
use serde_json::{Value, json};

pub(super) fn assignment() -> DecisionAssignment {
    let threshold = DecisionThreshold {
        probability_bps: 9000,
        confidence_bps: 9000,
    };
    DecisionAssignment {
        revision_id: ID.to_owned(),
        profile: DecisionProfile {
            model_id: ID.to_owned(),
            revision: "reviewed".to_owned(),
            api: DecisionApi::Typesafe,
            endpoint: "https://decision.fixture.test/v1".to_owned(),
            model: "decision-model".to_owned(),
            accepted_models: vec!["decision-model".to_owned()],
            capabilities: DecisionCapabilities {
                noul: true,
                choice: true,
                score: false,
            },
            input_max_bytes: 24576,
        },
        intent: IntentDecisionPolicy {
            questions_revision: "dt-routing-v1".to_owned(),
            intent: threshold,
            association: threshold,
            single_request_max_bps: 1000,
            multiple_request_min_bps: 9000,
        },
        goal: GoalDecisionPolicy {
            questions_revision: "dt-goal-v1".to_owned(),
            statement_min_bps: 9000,
            continue_action: threshold,
            wait: threshold,
            done: threshold,
        },
    }
}
pub(super) fn inputs() -> Value {
    let task = TaskContext {
        task_id: ID.into(),
        originating_item_id: ID.into(),
        target: TaskTarget::ApiOnly,
        generation: 7,
        stop_generation: 0,
        accepted_revision_id: ID.into(),
        objective: "Save accepted progress".into(),
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
    serde_json::to_value(TaskInputsResponse {
        source_admission_id: ID.to_owned(),
        accepted_revision_id: Some(ID.to_owned()),
        current_message: None,
        inputs: vec![],
        accepted_inputs: vec![],
        current_task: Some(task),
        execution_deadline: Some(deadline()),
        current_interpretation: None,
        tasks: vec![],
        next_cursor: None,
    })
    .unwrap()
}
pub(super) fn deadline() -> jiff::Timestamp {
    afd_core::clock::after(afd_core::clock::now(), std::time::Duration::from_mins(10))
}
pub(super) fn plan(input: &CheckpointPlanInput) -> CheckpointPlan {
    let deadline = deadline();
    CheckpointPlan {
        checkpoint_id: CHECKPOINT.to_owned(),
        generation: 1,
        base_id: None,
        mode: input.mode,
        repository: "api-only".to_owned(),
        format_version: input.format_version,
        toolbox_digest: input.toolbox_digest.clone(),
        page_ids: vec![PAGE.to_owned()],
        base_page_ids: vec![],
        base_manifest_root: None,
        base_commit_oid: None,
        manifest_root: None,
        history_digest: None,
        covered_input_sequence: 0,
        effect_watermark: 0,
        expires_at: deadline,
        pin: CheckpointPin {
            id: ID.to_owned(),
            resource_version: 1,
            operation_deadline: deadline,
            expires_at: deadline,
        },
        replayed: false,
    }
}
pub(super) fn decision(input: &DecisionReservationInput) -> Value {
    assert!(matches!(input.assessment, DecisionAssessment::Goal { .. }));
    assert_eq!(input.source_admission_id, ID);
    assert_eq!(
        input
            .task
            .as_ref()
            .map(|task| (task.task_id.as_str(), task.generation)),
        Some((ID, 7))
    );
    let answers = std::collections::BTreeMap::from([(
        "next_action".to_owned(),
        DecisionAnswer::Choice {
            choice: "WAIT".to_owned(),
            confidence: Some(1.0),
            probabilities: std::collections::BTreeMap::from([
                ("DONE".to_owned(), 0.0),
                ("WAIT".to_owned(), 1.0),
                ("CONTINUE".to_owned(), 0.0),
            ]),
        },
    )]);
    let recorded = DecisionRecorded {
        request: DecisionRequest {
            call_id: input.operation_id.clone(),
            purpose: DecisionPurpose::Goal,
            state: input.evidence.clone(),
            questions: vec![next_action()],
        },
        result: DecisionResult {
            reported_model: Some("decision-model".to_owned()),
            usage: Some(DecisionUsage {
                input_tokens: 10,
                output_tokens: 2,
                cached_input_tokens: 0,
                cache_write_tokens: 0,
            }),
            result: DecisionOutcome::Answered { answers },
        },
        disposition: DecisionRecordDisposition::Current,
    };
    validate_recorded(&recorded);
    json!({"status":"existing","call_id":input.operation_id,"state":"known","recorded":recorded})
}

fn next_action() -> NamedQuestion {
    NamedQuestion {
        name: "next_action".to_owned(),
        question: DecisionQuestion::Choice {
            instructions: "Choose the next action supported by the observed task state.".to_owned(),
            options: ["DONE", "WAIT", "CONTINUE"]
                .into_iter()
                .map(|value| ChoiceOption {
                    value: value.to_owned(),
                    description: format!("The evidence supports {value}"),
                })
                .collect(),
        },
    }
}
fn validate_recorded(recorded: &DecisionRecorded) {
    use garde::Validate as _;
    recorded.request.validate().unwrap();
    DecisionReceiptInput {
        fencing_token: super::FENCING,
        receipt: DecisionReceipt::Provider {
            result: recorded.result.clone(),
        },
    }
    .validate()
    .unwrap();
    let answers = match &recorded.result.result {
        DecisionOutcome::Answered { answers } => Some(answers),
        _ => None,
    }
    .unwrap();
    assert_eq!(answers.len(), recorded.request.questions.len());
    for question in &recorded.request.questions {
        assert!(answer_matches(
            &question.question,
            answers.get(&question.name).unwrap()
        ));
    }
}
