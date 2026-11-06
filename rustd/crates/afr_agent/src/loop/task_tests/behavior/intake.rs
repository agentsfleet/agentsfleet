//! Behavior at the public root execution boundary.

#![expect(
    clippy::expect_used,
    reason = "test fixture: poisoned state must fail the test"
)]

use super::intake_fixture::*;
use super::*;

#[tokio::test]
async fn accepted_intent_survives_lost_binding_acknowledgement_without_reclassification()
-> crate::Result<()> {
    for lose_acknowledgement in [false, true] {
        let plan = serde_json::json!([proposed("steer", Some(SOURCE), 0, 9)]).to_string();
        let script = Script::new([vec![say(&plan), ended(false)]]);
        let mut native = native("repair it", [route("steer", "task_1", 0.0)]);
        native.lose_acknowledgement = lose_acknowledgement;
        for _ in 0..2 {
            let prepared = prepare(&script, &native).await?;
            assert!(prepared.allow_actions);
            assert!(prepared.execution_deadline.is_some());
            assert_eq!(
                prepared
                    .inputs
                    .current_interpretation
                    .as_ref()
                    .map(|value| value.message.as_str()),
                Some("repair it")
            );
        }
        assert_single_binding(&native)?;
        assert_eq!(
            native
                .decisions
                .lock()
                .expect("fixture mutex is unpoisoned")
                .len(),
            1
        );
        assert_eq!(script.sent().len(), 1);
        assert!(
            script
                .sent()
                .first()
                .is_some_and(|sent| sent.tools.is_empty() && sent.hosted.is_empty())
        );
    }
    Ok(())
}

#[tokio::test]
async fn mixed_source_plan_requires_separate_decisions_for_every_ordered_item() -> crate::Result<()>
{
    let plan = serde_json::json!([
        proposed("conversation", None, 0, 2),
        proposed("steer", Some(SOURCE), 3, 12)
    ])
    .to_string();
    let script = Script::new([vec![say(&plan), ended(false)]]);
    let native = native(
        "hi repair it",
        [
            route("steer", "task_1", 1.0),
            route("conversation", "unclear", 0.0),
            route("steer", "task_1", 0.0),
        ],
    );
    let prepared = prepare(&script, &native).await?;
    assert!(prepared.allow_actions);
    let bindings = native.bindings.lock().expect("fixture mutex is unpoisoned");
    let binding = bindings.first().ok_or_else(crate::decision::unavailable)?;
    assert_eq!(binding.items.len(), 2);
    let decisions = native
        .decisions
        .lock()
        .expect("fixture mutex is unpoisoned");
    assert_eq!(decisions.len(), 3);
    for (item, decision) in binding.items.iter().zip(decisions.iter().skip(1)) {
        assert_eq!(
            item.decision_call_id.as_deref(),
            Some(decision.operation_id.as_str())
        );
    }

    let mut decisions = decisions.iter();
    assert!(matches!(
        decisions.next().map(|call| &call.assessment),
        Some(DecisionAssessment::Message)
    ));
    for (ordinal, start, end) in [(0, 0, 2), (1, 3, 12)] {
        let decision = decisions.next().ok_or_else(crate::decision::unavailable)?;
        assert!(
            matches!(&decision.assessment,DecisionAssessment::Item { ordinal: actual, interpretation_digest, source_start, source_end }
            if *actual == ordinal && interpretation_digest == &binding.plan_digest && *source_start == start && *source_end == end)
        );
    }
    Ok(())
}

#[tokio::test]
async fn uncertain_or_incomplete_intent_never_reaches_native_binding() -> crate::Result<()> {
    for (multiple, plan, action, association) in rejected_plans() {
        let script = Script::new([vec![say(&plan), ended(false)]]);
        let native = native("repair it", [route(action, association, multiple)]);
        let prepared = prepare(&script, &native).await?;
        assert!(!prepared.allow_actions);
        assert!(prepared.inputs.current_interpretation.is_none());
        assert!(
            native
                .bindings
                .lock()
                .expect("fixture mutex is unpoisoned")
                .is_empty()
        );
        assert_eq!(
            native
                .decisions
                .lock()
                .expect("fixture mutex is unpoisoned")
                .len(),
            1
        );
    }
    Ok(())
}

fn rejected_plans() -> Vec<(f64, String, &'static str, &'static str)> {
    vec![
        (
            0.5,
            serde_json::json!([proposed("steer", Some(SOURCE), 0, 9)]).to_string(),
            "steer",
            "task_1",
        ),
        (0.0, "not json".to_owned(), "steer", "task_1"),
        (
            0.0,
            serde_json::json!([proposed("steer", Some(SOURCE), 0, 6)]).to_string(),
            "steer",
            "task_1",
        ),
        (
            0.0,
            serde_json::json!([proposed("steer", Some(SOURCE), 0, 9)]).to_string(),
            "conversation",
            "task_1",
        ),
        (
            0.0,
            serde_json::json!([proposed("steer", Some(PENDING), 0, 9)]).to_string(),
            "steer",
            "task_1",
        ),
        (
            0.0,
            serde_json::json!([proposed("conversation", Some(SOURCE), 0, 9)]).to_string(),
            "conversation",
            "unclear",
        ),
    ]
}

fn assert_single_binding(native: &IntakeNative) -> crate::Result<()> {
    let bindings = native.bindings.lock().expect("fixture mutex is unpoisoned");
    assert_eq!(bindings.len(), 1);
    let binding = bindings.first().ok_or_else(crate::decision::unavailable)?;
    assert_eq!(binding.source_admission_id, SOURCE);
    assert_eq!(binding.accepted_revision_id, SOURCE);
    let decisions = native
        .decisions
        .lock()
        .expect("fixture mutex is unpoisoned");
    assert_eq!(
        binding.message_decision_call_id,
        decisions
            .first()
            .ok_or_else(crate::decision::unavailable)?
            .operation_id
    );
    assert_eq!(binding.plan_digest.len(), 64);
    assert_eq!(binding.items.len(), 1);
    assert_eq!(
        binding.items.first().map(|item| item.proposed.action),
        Some(TaskInputAction::Steer)
    );
    assert!(
        binding
            .items
            .first()
            .is_some_and(|item| item.decision_call_id.is_none())
    );
    Ok(())
}
