//! Schema-valid daemon decisions distinguish accounting facts from usable answers.
use super::CALL;
use afd_wire::{decision::*, task::*};
use serde_json::{Value, json};

pub(super) fn assignment() -> DecisionAssignment {
    serde_json::from_value(
        super::super::dispatch("typesafe", "goal")
            .pointer("/dispatch/assignment")
            .unwrap()
            .clone(),
    )
    .unwrap()
}

pub(super) fn inputs() -> TaskInputsResponse<'static> {
    let deadline =
        afd_core::clock::after(afd_core::clock::now(), std::time::Duration::from_mins(5));
    TaskInputsResponse {
        source_admission_id: CALL.into(),
        accepted_revision_id: Some(CALL.into()),
        current_message: None,
        inputs: vec![],
        accepted_inputs: vec![],
        current_interpretation: None,
        tasks: vec![],
        next_cursor: None,
        execution_deadline: Some(deadline),
        current_task: Some(TaskContext {
            task_id: CALL.into(),
            originating_item_id: CALL.into(),
            target: TaskTarget::ApiOnly,
            generation: 1,
            stop_generation: 0,
            accepted_revision_id: CALL.into(),
            objective: "Retain the accepted repair".into(),
            goal_digest: "a".repeat(64).into(),
            questions: vec![],
            state: TaskState::Running,
            covered_input_sequence: 0,
            input_sequence: 0,
            effect_cursor: 0,
            confirmed_effect_watermark: 0,
            checkpoint_id: None,
            checkpoint_generation: None,
            expires_at: deadline,
            limits: TaskLimits::default(),
            usage: TaskUsage::default(),
        }),
    }
}

pub(super) fn reservation(mode: &str, request: &DecisionReservationInput) -> Value {
    if matches!(
        mode,
        "empty_credential" | "invalid_header" | "mismatched_receipt"
    ) {
        let mut value = super::super::dispatch("typesafe", "goal");
        *value.get_mut("call_id").unwrap() = json!(request.operation_id);
        *value.pointer_mut("/dispatch/request/call_id").unwrap() = json!(request.operation_id);
        *value.pointer_mut("/dispatch/api_key").unwrap() = json!(match mode {
            "invalid_header" => "invalid\nheader",
            _ => "",
        });
        return value;
    }
    if !matches!(mode, "refused" | "stale") {
        return json!({"status":"existing","call_id":request.operation_id,"state":mode});
    }
    let recorded = recorded(mode, request);
    json!({"status":"existing","call_id":request.operation_id,"state":"known","recorded":recorded})
}

fn recorded(mode: &str, request: &DecisionReservationInput) -> DecisionRecorded {
    let recorded = DecisionRecorded {
        request: DecisionRequest {
            call_id: request.operation_id.clone(),
            purpose: DecisionPurpose::Goal,
            state: request.evidence.clone(),
            questions: vec![next_action()],
        },
        result: DecisionResult {
            reported_model: Some("fixture-model".into()),
            usage: Some(DecisionUsage {
                input_tokens: 10,
                output_tokens: 2,
                cached_input_tokens: 0,
                cache_write_tokens: 0,
            }),
            result: outcome(mode),
        },
        disposition: if mode == "stale" {
            DecisionRecordDisposition::Stale
        } else {
            DecisionRecordDisposition::Unusable
        },
    };
    garde::Validate::validate(&recorded.request).unwrap();
    recorded
}

fn next_action() -> NamedQuestion {
    NamedQuestion {
        name: "next_action".into(),
        question: DecisionQuestion::Choice {
            instructions: "Select the next supported action".into(),
            options: ["DONE", "WAIT", "CONTINUE"]
                .into_iter()
                .map(|value| ChoiceOption {
                    value: value.into(),
                    description: format!("Evidence supports {value}"),
                })
                .collect(),
        },
    }
}

fn outcome(mode: &str) -> DecisionOutcome {
    if mode == "refused" {
        DecisionOutcome::Refused
    } else {
        DecisionOutcome::Answered {
            answers: std::collections::BTreeMap::from([(
                "next_action".into(),
                DecisionAnswer::Choice {
                    choice: "DONE".into(),
                    confidence: Some(1.0),
                    probabilities: std::collections::BTreeMap::from([
                        ("DONE".into(), 1.0),
                        ("WAIT".into(), 0.0),
                        ("CONTINUE".into(), 0.0),
                    ]),
                },
            )]),
        }
    }
}

/// The tiny transport fixture has one context token; real Loop execution needs
/// room for its accepted goal, initial instructions and one model response.
pub(super) fn lease() -> afd_wire::lease::LeasePayload<'static> {
    let mut lease = crate::test_support::lease(super::LEASE_ID, CALL);
    lease.policy.tools.clear();
    lease.policy.context.context_cap_tokens = 64_000;
    lease.policy.context.tool_window = 64;
    lease.policy.model_context_tokens = 64_000;
    lease.policy.context.memory_checkpoint_every = 100;
    lease.durable_tasks = true;
    lease.decision = Some(Box::new(assignment()));
    lease
}
