//! Native goal receipts determine whether root work continues or can be reported.

#![expect(
    clippy::expect_used,
    reason = "test fixture: poisoned state must fail the test"
)]

use super::*;
use afd_wire::decision::*;
use afr_tools::verbs::DecisionEvaluation;
use std::collections::{BTreeMap, VecDeque};
use std::sync::Mutex;

mod evidence;
mod intake;
mod intake_fixture;
mod publication;
mod publication_fixture;
mod recovery;
mod recovery_fixture;
mod replies;
mod replies_fixture;

#[derive(Debug)]
struct Goals {
    inputs: TaskInputsResponse<'static>,
    replies: Mutex<VecDeque<DecisionEvaluation>>,
    requests: Mutex<Vec<DecisionReservationInput>>,
}

#[async_trait::async_trait]
impl LeaseVerbs for Goals {
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
        Ok(self.inputs.clone())
    }
    async fn task_effects(&self, _: TaskEffectsQuery) -> Result<TaskEffectsPage, Unanswered> {
        Ok(TaskEffectsPage {
            effects: vec![],
            next_cursor: None,
        })
    }
    async fn task_decision(
        &self,
        input: DecisionReservationInput,
    ) -> Result<DecisionEvaluation, Unanswered> {
        assert!(matches!(input.assessment, DecisionAssessment::Goal { .. }));
        assert_eq!(input.source_admission_id, SOURCE);
        assert_eq!(
            input.task,
            Some(DecisionTaskVersion {
                task_id: SOURCE.to_owned(),
                generation: 1
            })
        );
        let call_id = input.operation_id.clone();
        self.requests
            .lock()
            .expect("fixture mutex is unpoisoned")
            .push(input);
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
}

fn assigned_lease(tools: &[&str], context: u32) -> afd_wire::lease::LeasePayload<'static> {
    let mut lease = lease(tools, unbounded());
    lease.durable_tasks = true;
    lease.policy.model_context_tokens = context;
    lease.decision = Some(Box::new(assignment()));
    lease
}

fn assignment() -> DecisionAssignment {
    let threshold = DecisionThreshold {
        probability_bps: 9000,
        confidence_bps: 9000,
    };
    DecisionAssignment {
        revision_id: SOURCE.to_owned(),
        profile: DecisionProfile {
            model_id: SOURCE.to_owned(),
            revision: "reviewed".to_owned(),
            api: DecisionApi::Typesafe,
            endpoint: "https://decision.example.test/v1".to_owned(),
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

fn evaluation(choice: &str, confidence: Option<f64>, statement: f64) -> DecisionEvaluation {
    let answers = BTreeMap::from([
        (
            "next_action".to_owned(),
            choice_answer(choice, confidence, &["DONE", "WAIT", "CONTINUE"]),
        ),
        (
            "verified".to_owned(),
            DecisionAnswer::Noul { noul: statement },
        ),
    ]);
    DecisionEvaluation {
        call_id: PENDING.to_owned(),
        state: DecisionCallState::Known,
        recorded: Some(Box::new(DecisionRecorded {
            request: DecisionRequest {
                call_id: PENDING.to_owned(),
                purpose: DecisionPurpose::Goal,
                state: serde_json::json!({"task_id": SOURCE}),
                questions: vec![
                    question(),
                    choice_question("next_action", &["DONE", "WAIT", "CONTINUE"]),
                ],
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
        })),
    }
}

fn question() -> NamedQuestion {
    NamedQuestion {
        name: "verified".to_owned(),
        question: DecisionQuestion::Noul {
            instructions: "Has the failure been investigated?".to_owned(),
            criteria: NoulCriteria {
                yes: "The investigation is recorded".to_owned(),
                no: "Evidence is missing".to_owned(),
            },
        },
    }
}

fn goals(replies: impl IntoIterator<Item = DecisionEvaluation>) -> Goals {
    let mut inputs = inputs();
    let mut current = task();
    current.questions = vec![question()];
    inputs.current_task = Some(current);
    Goals {
        inputs,
        replies: Mutex::new(replies.into_iter().collect()),
        requests: Mutex::new(vec![]),
    }
}

#[tokio::test]
async fn complete_and_wait_reports_retain_native_goal_proof_and_private_history()
-> crate::Result<()> {
    for (choice, disposition) in [
        ("DONE", TaskDisposition::Complete),
        ("WAIT", TaskDisposition::Waiting),
    ] {
        let script = Script::new([vec![say("Investigation recorded."), ended(false)]]);
        let engine = super::super::tests::engine(vec![], &script);
        let lease = assigned_lease(&[], 16_384);
        let native = goals([evaluation(choice, Some(1.0), 1.0)]);
        let prepared = PreparedTask {
            execution_deadline: native.inputs.execution_deadline,
            inputs: native.inputs.clone(),
            allow_actions: true,
        };
        let output = run(&engine, &lease, &native, &prepared).await?;
        let report = output.task.ok_or_else(crate::decision::unavailable)?;
        assert_eq!(report.disposition, disposition);

        assert_eq!(report.task.generation, 1);
        assert_eq!(report.covered_input_sequence, 0);
        assert_eq!(report.effect_watermark, 0);
        assert_eq!(output.result.content, "Investigation recorded.");
        assert!(output.recovery.is_some());
        let requests = native.requests.lock().expect("fixture mutex is unpoisoned");
        assert_eq!(requests.len(), 1);
        assert_eq!(
            report.decision_call_id.as_deref(),
            requests
                .first()
                .map(|request| request.operation_id.as_str())
        );
        assert_eq!(
            requests.first().map(|request| &request.evidence["answer"]),
            Some(&serde_json::json!("Investigation recorded."))
        );
    }
    Ok(())
}

fn refused_goals() -> Vec<DecisionEvaluation> {
    let mut stale = evaluation("DONE", Some(1.0), 1.0);
    if let Some(recorded) = stale.recorded.as_mut() {
        recorded.disposition = DecisionRecordDisposition::Stale;
    }
    let mut unknown = evaluation("DONE", Some(1.0), 1.0);
    unknown.recorded = None;
    let mut refused = evaluation("DONE", Some(1.0), 1.0);
    if let Some(recorded) = refused.recorded.as_mut() {
        recorded.result.result = DecisionOutcome::Refused;
    }
    let mut improbable = evaluation("DONE", Some(1.0), 1.0);
    if let Some(recorded) = improbable.recorded.as_mut()
        && let DecisionOutcome::Answered { answers } = &mut recorded.result.result
        && let Some(DecisionAnswer::Choice { probabilities, .. }) = answers.get_mut("next_action")
    {
        probabilities.insert("DONE".to_owned(), 0.89);
        probabilities.insert("CONTINUE".to_owned(), 0.11);
    }
    vec![
        evaluation("DONE", None, 1.0),
        evaluation("DONE", Some(0.89), 1.0),
        evaluation("DONE", Some(1.0), 0.89),
        stale,
        unknown,
        refused,
        improbable,
    ]
}

#[tokio::test]
async fn untrusted_goal_results_never_authorize_completion() -> crate::Result<()> {
    for result in refused_goals() {
        let script = Script::new([vec![say("I claim this is complete."), ended(false)]]);
        let engine = super::super::tests::engine(vec![], &script);
        let lease = assigned_lease(&[], 16_384);
        let native = goals([result]);
        let prepared = PreparedTask {
            execution_deadline: native.inputs.execution_deadline,
            inputs: native.inputs.clone(),
            allow_actions: true,
        };
        let output = run(&engine, &lease, &native, &prepared).await?;
        assert!(output.task.is_none());
        assert!(output.recovery.is_some());
        assert!(matches!(
            output.result.outcome,
            crate::ResultOutcome::Failed(_)
        ));
        assert!(output.result.content.is_empty());
        assert_eq!(
            native
                .requests
                .lock()
                .expect("fixture mutex is unpoisoned")
                .len(),
            1
        );
    }
    Ok(())
}

#[tokio::test]
async fn continue_requires_another_action_group_and_a_fresh_goal_receipt() -> crate::Result<()> {
    let script = Script::new([
        vec![say("Still investigating."), ended(false)],
        vec![say("Investigation recorded."), ended(false)],
    ]);
    let engine = super::super::tests::engine(vec![], &script);
    let lease = assigned_lease(&[], 16_384);
    let native = goals([
        evaluation("CONTINUE", Some(1.0), 0.0),
        evaluation("DONE", Some(1.0), 1.0),
    ]);
    let prepared = PreparedTask {
        execution_deadline: native.inputs.execution_deadline,
        inputs: native.inputs.clone(),
        allow_actions: true,
    };
    let output = run(&engine, &lease, &native, &prepared).await?;
    assert_eq!(
        output
            .task
            .ok_or_else(crate::decision::unavailable)?
            .disposition,
        TaskDisposition::Complete
    );
    assert_eq!(output.result.content, "Investigation recorded.");
    assert_eq!(script.sent().len(), 2);
    let requests = native.requests.lock().expect("fixture mutex is unpoisoned");
    assert_eq!(requests.len(), 2);
    let mut requests = requests.iter();
    let first = requests.next().ok_or_else(crate::decision::unavailable)?;
    let second = requests.next().ok_or_else(crate::decision::unavailable)?;
    assert_ne!(first.operation_id, second.operation_id);
    assert_eq!(first.evidence["answer"], "Still investigating.");
    assert_eq!(second.evidence["answer"], "Investigation recorded.");
    Ok(())
}

mod decision_fixture;
use decision_fixture::{choice_answer, choice_question};
