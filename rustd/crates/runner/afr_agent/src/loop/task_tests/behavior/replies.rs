//! Ordered source reply outcomes through the public root execution boundary.

#![expect(
    clippy::expect_used,
    reason = "test fixture: poisoned state must fail the test"
)]

use super::replies_fixture::*;
use super::*;

#[tokio::test]
async fn conversational_and_status_items_answer_their_source_without_action_requests()
-> crate::Result<()> {
    let native = source(vec![
        item(ITEM, TaskInputAction::Conversation, false),
        item(PENDING, TaskInputAction::Status, false),
    ]);
    let script = Script::new([
        vec![say("Hello."), ended(false)],
        vec![say("The investigation is running."), ended(false)],
    ]);
    let output = execute(&script, &native, false).await?;
    assert_eq!(
        output.result.content,
        "Hello.\n\nThe investigation is running."
    );
    assert!(output.task.is_none());
    assert!(output.recovery.is_none());
    assert!(script.sent().iter().all(|request| request.tools.is_empty()));
    let receipts = native.receipts.lock().expect("fixture mutex is unpoisoned");
    assert_eq!(receipts.len(), 2);
    assert!(
        receipts
            .first()
            .is_some_and(|receipt| receipt.task.is_none())
    );
    assert_eq!(
        receipts.last().and_then(|receipt| receipt.task.as_ref()),
        Some(&DecisionTaskVersion {
            task_id: SOURCE.to_owned(),
            generation: 1
        })
    );
    assert!(
        native
            .goals
            .requests
            .lock()
            .expect("fixture mutex is unpoisoned")
            .is_empty()
    );
    Ok(())
}

#[tokio::test]
async fn mixed_source_keeps_replies_beside_active_task_result_in_original_order()
-> crate::Result<()> {
    let native = source(vec![
        item(PENDING, TaskInputAction::Conversation, false),
        item(ITEM, TaskInputAction::NewWork, true),
        item(SOURCE, TaskInputAction::Conversation, false),
    ]);
    let script = Script::new([
        vec![say("Hello."), ended(false)],
        vec![say("Second answer."), ended(false)],
        vec![say("Work needs checks."), ended(false)],
    ]);
    let output = execute(&script, &native, true).await?;
    assert_eq!(
        output.result.content,
        "Hello.\n\nWork needs checks.\n\nSecond answer."
    );
    assert_eq!(
        output
            .task
            .ok_or_else(crate::decision::unavailable)?
            .disposition,
        TaskDisposition::Waiting
    );
    assert!(output.recovery.is_some());
    let receipts = native.receipts.lock().expect("fixture mutex is unpoisoned");
    assert_eq!(
        receipts
            .iter()
            .map(|receipt| receipt.item_id.as_str())
            .collect::<Vec<_>>(),
        vec![PENDING, ITEM, SOURCE]
    );
    assert!(
        receipts
            .iter()
            .find(|receipt| receipt.item_id == ITEM)
            .and_then(|receipt| receipt.response_text.as_deref())
            .is_some_and(|text| text.contains("accepted into the task"))
    );
    assert_eq!(script.sent().len(), 3);
    Ok(())
}

#[tokio::test]
async fn empty_reply_or_wrong_native_acknowledgement_does_not_claim_the_source_answered()
-> crate::Result<()> {
    for (answer, wrong_ack, receipt_count) in [("  ", false, 0), ("Hello.", true, 1)] {
        let mut native = source(vec![item(ITEM, TaskInputAction::Conversation, false)]);
        native.wrong_ack = wrong_ack;
        let script = Script::new([vec![say(answer), ended(false)]]);
        let output = execute(&script, &native, false).await?;
        assert!(output.task.is_none());
        assert!(output.result.content.is_empty());
        assert_eq!(
            output.result.outcome,
            crate::ResultOutcome::Failed(crate::Failure {
                class: None,
                detail: "the run was stopped before it finished".into(),
            })
        );
        assert_eq!(
            native
                .receipts
                .lock()
                .expect("fixture mutex is unpoisoned")
                .len(),
            receipt_count
        );
        assert!(
            native
                .inputs
                .lock()
                .expect("fixture mutex is unpoisoned")
                .current_interpretation
                .as_ref()
                .is_some_and(|source| source.items.iter().all(|item| item.response_text.is_none()))
        );
        assert!(
            native
                .goals
                .requests
                .lock()
                .expect("fixture mutex is unpoisoned")
                .is_empty()
        );
    }
    Ok(())
}

#[tokio::test]
async fn another_tasks_status_reply_uses_its_listed_native_generation() -> crate::Result<()> {
    let native = source(vec![item(ITEM, TaskInputAction::Status, false)]);
    use_listed_task(&native);
    let script = Script::new([vec![say("The task is waiting for review."), ended(false)]]);
    let output = execute(&script, &native, false).await?;
    assert_eq!(output.result.content, "The task is waiting for review.");
    assert!(output.task.is_none());
    assert!(output.recovery.is_none());
    let receipts = native.receipts.lock().expect("fixture mutex is unpoisoned");
    assert_eq!(receipts.len(), 1);
    assert_eq!(
        receipts.first().and_then(|receipt| receipt.task.as_ref()),
        Some(&DecisionTaskVersion {
            task_id: SOURCE.to_owned(),
            generation: 4
        })
    );
    assert!(
        native
            .goals
            .requests
            .lock()
            .expect("fixture mutex is unpoisoned")
            .is_empty()
    );
    Ok(())
}

fn use_listed_task(native: &Replies) {
    let mut inputs = native.inputs.lock().expect("fixture mutex is unpoisoned");
    inputs.current_task = None;
    inputs.tasks = vec![TaskInputCandidate {
        task_id: SOURCE.to_owned(),
        title: "Review the repair".to_owned(),
        state: TaskState::Waiting,
        generation: 4,
    }];
}

// Value: a source reply writes no more than the route's own output limit,
// and never more than 4,096 tokens.
#[tokio::test]
async fn a_source_reply_writes_within_its_route_and_at_most_4096() -> crate::Result<()> {
    for (limit, asked) in [(100, 100), (100_000, 4096)] {
        let native = source(vec![item(ITEM, TaskInputAction::Conversation, false)]);
        let script = Script::new([vec![say("Hello."), ended(false)]]);
        let engine = super::super::super::tests::engine(vec![], &script);
        let mut this_run = assigned_run(&[], 16_384);
        this_run.policy.inference = Some(af_wire::inference::ModelInference {
            api: af_wire::inference::InferenceApi::Messages,
            base_url: "https://models.example.com/v1".into(),
            dialect: None,
            max_output_tokens: limit,
        });
        let inputs = native
            .inputs
            .lock()
            .expect("fixture mutex is unpoisoned")
            .clone();
        let prepared = PreparedTask {
            execution_deadline: inputs.execution_deadline,
            inputs,
            allow_actions: false,
        };

        let output = run(&engine, &this_run, &native, &prepared).await?;

        assert_eq!(output.result.content, "Hello.");
        assert_eq!(
            script
                .sent()
                .first()
                .and_then(|sent| sent.max_output_tokens),
            Some(asked)
        );
    }
    Ok(())
}
