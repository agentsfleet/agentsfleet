//! Which goal results authorize a task report: held statements complete the
//! task whatever the next action says, and an untrusted record or a next
//! action below its threshold reports nothing.

use super::*;

/// The record `evaluation`, built by [`evaluation`], always carries.
fn recorded(evaluation: &mut DecisionEvaluation) -> &mut DecisionRecorded {
    evaluation
        .recorded
        .as_deref_mut()
        .expect("the fixture's evaluation is recorded")
}

/// Results that authorize no report. A statement below the bar leaves the next
/// action to decide, so those cases test its thresholds; a record nobody may
/// trust authorizes nothing, even with every statement held.
fn refused_goals() -> Vec<DecisionEvaluation> {
    let mut stale = evaluation("DONE", Some(1.0), 1.0);
    recorded(&mut stale).disposition = DecisionRecordDisposition::Stale;
    let mut unknown = evaluation("DONE", Some(1.0), 1.0);
    unknown.recorded = None;
    let mut refused = evaluation("DONE", Some(1.0), 1.0);
    recorded(&mut refused).result.result = DecisionOutcome::Refused;
    let mut improbable = evaluation("WAIT", Some(1.0), 0.0);
    recorded(&mut improbable).result.result = DecisionOutcome::Answered {
        answers: BTreeMap::from([
            (
                "next_action".to_owned(),
                DecisionAnswer::Choice {
                    choice: "WAIT".to_owned(),
                    confidence: Some(1.0),
                    probabilities: BTreeMap::from([
                        ("DONE".to_owned(), 0.0),
                        ("WAIT".to_owned(), 0.89),
                        ("CONTINUE".to_owned(), 0.11),
                    ]),
                },
            ),
            ("verified".to_owned(), DecisionAnswer::Noul { noul: 0.0 }),
        ]),
    };
    let mut unchosen = evaluation("WAIT", Some(1.0), 0.0);
    recorded(&mut unchosen).result.result = DecisionOutcome::Answered {
        answers: BTreeMap::from([
            ("next_action".to_owned(), DecisionAnswer::Noul { noul: 1.0 }),
            ("verified".to_owned(), DecisionAnswer::Noul { noul: 0.0 }),
        ]),
    };
    vec![
        evaluation("WAIT", None, 0.0),
        evaluation("WAIT", Some(0.89), 0.0),
        evaluation("ESCALATE", Some(1.0), 0.0),
        unchosen,
        // DONE settles nothing once a statement falls below the bar.
        evaluation("DONE", Some(1.0), 0.89),
        stale,
        unknown,
        refused,
        improbable,
    ]
}

#[tokio::test]
async fn untrusted_goal_results_authorize_no_report() -> crate::Result<()> {
    for result in refused_goals() {
        let script = Script::new([vec![say("I claim this is complete."), ended(false)]]);
        let engine = super::super::super::tests::engine(vec![], &script);
        let this_run = assigned_run(&[], 16_384);
        let native = goals([result]);
        let prepared = PreparedDurableTask {
            execution_deadline: native.inputs.execution_deadline,
            inputs: native.inputs.clone(),
        };
        let output = run(&engine, &this_run, &native, &prepared).await?;
        assert!(output.task.is_none());
        assert!(output.recovery.is_some());
        assert_eq!(
            output.result.outcome,
            crate::ResultOutcome::Failed(crate::Failure {
                class: None,
                detail: "the run was stopped before it finished".into(),
            })
        );
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

/// A goal check whose next action picks `choice` at `probability` and
/// `confidence`, the rest split over the other options, and whose one
/// statement scores `statement`.
fn judged(choice: &str, probability: f64, confidence: f64, statement: f64) -> DecisionEvaluation {
    let rest = (1.0 - probability) / 2.0;
    let mut judged = evaluation(choice, Some(confidence), statement);
    recorded(&mut judged).result.result = DecisionOutcome::Answered {
        answers: BTreeMap::from([
            (
                "next_action".to_owned(),
                DecisionAnswer::Choice {
                    choice: choice.to_owned(),
                    confidence: Some(confidence),
                    probabilities: ["DONE", "WAIT", "CONTINUE"]
                        .into_iter()
                        .map(|option| {
                            let share = if option == choice { probability } else { rest };
                            (option.to_owned(), share)
                        })
                        .collect(),
                },
            ),
            (
                "verified".to_owned(),
                DecisionAnswer::Noul { noul: statement },
            ),
        ]),
    };
    judged
}

#[tokio::test]
async fn statements_that_hold_complete_the_task_whatever_the_next_action_says() -> crate::Result<()>
{
    // A dev task's checks: two below the bar continue, then its statement holds
    // at 0.97 while DONE has probability 0.43 and confidence 0.24. A held
    // statement also outranks a CONTINUE that clears its own threshold.
    let dev = vec![
        judged("CONTINUE", 0.96, 0.95, 0.03),
        judged("CONTINUE", 0.97, 0.96, 0.26),
        judged("DONE", 0.43, 0.24, 0.97),
    ];
    for checks in [dev, vec![judged("CONTINUE", 1.0, 1.0, 0.97)]] {
        let groups = checks.len();
        let script = Script::new((0..groups).map(|_| vec![say("Recalled three."), ended(false)]));
        let engine = super::super::super::tests::engine(vec![], &script);
        let this_run = assigned_run(&[], 16_384);
        let native = goals(checks);
        let prepared = PreparedDurableTask {
            execution_deadline: native.inputs.execution_deadline,
            inputs: native.inputs.clone(),
        };
        let output = run(&engine, &this_run, &native, &prepared).await?;
        let report = output.task.ok_or_else(crate::decision::unavailable)?;
        assert_eq!(report.disposition, DurableTaskDisposition::Complete);
        assert_eq!(output.result.content, "Recalled three.");
        assert_eq!(script.sent().len(), groups, "each earlier check continued");
        let requests = native.requests.lock().expect("fixture mutex is unpoisoned");
        assert_eq!(requests.len(), groups);
        assert_eq!(
            report.decision_call_id.as_deref(),
            requests.last().map(|request| request.operation_id.as_str())
        );
    }
    Ok(())
}
