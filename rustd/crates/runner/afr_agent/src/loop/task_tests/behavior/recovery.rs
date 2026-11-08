//! Behavior at the public root execution boundary.

#![expect(
    clippy::expect_used,
    reason = "test fixture: poisoned state must fail the test"
)]

use super::recovery_fixture::*;
use super::*;

#[tokio::test]
async fn every_accepted_input_page_is_applied_and_answered_before_action() -> crate::Result<()> {
    let mut native = receipts();
    native.inputs = vec![accepted(ITEM, 1), accepted(PENDING, 2)];
    let script = Script::new([vec![say("Ready for validation."), ended(false)]]);
    let output = execute(&script, &native, None).await?;
    assert_eq!(
        output
            .task
            .ok_or_else(crate::decision::unavailable)?
            .covered_input_sequence,
        2
    );
    let recorded = native.recorded.lock().expect("fixture mutex is unpoisoned");
    assert_eq!(
        recorded
            .iter()
            .map(|receipt| (receipt.item_id.as_str(), receipt.mode))
            .collect::<Vec<_>>(),
        vec![
            (ITEM, TaskInputMode::ApplyHere),
            (ITEM, TaskInputMode::Answered),
            (PENDING, TaskInputMode::ApplyHere),
            (PENDING, TaskInputMode::Answered)
        ]
    );
    assert!(
        recorded
            .iter()
            .filter(|receipt| receipt.mode == TaskInputMode::Answered)
            .all(|receipt| receipt
                .response_text
                .as_deref()
                .is_some_and(|text| text.contains("incorporated")))
    );
    let sent = script.sent();
    let messages = &sent
        .first()
        .ok_or_else(crate::decision::unavailable)?
        .messages;
    assert_eq!(messages.iter().filter(|message| matches!(message,af_providers::Message::User(text) if text.contains("accepted_task_input"))).count(),2);
    Ok(())
}

#[tokio::test]
async fn applied_input_is_restored_without_duplicate_application_or_reply() -> crate::Result<()> {
    let mut native = receipts();
    let mut input = accepted(ITEM, 1);
    input.applied = true;
    input.response_text = Some("Already acknowledged".to_owned());
    native.inputs = vec![input];
    let script = Script::new([vec![say("Ready for validation."), ended(false)]]);
    let output = execute(&script, &native, None).await?;
    assert_eq!(
        output
            .task
            .ok_or_else(crate::decision::unavailable)?
            .covered_input_sequence,
        1
    );
    assert!(
        native
            .recorded
            .lock()
            .expect("fixture mutex is unpoisoned")
            .is_empty()
    );
    assert!(script.sent().first().is_some_and(|sent| sent.messages.iter().any(|message| matches!(message,af_providers::Message::User(text) if text.contains("Do not remove tests")))));
    Ok(())
}

#[tokio::test]
async fn skipped_input_sequence_or_wrong_acknowledgement_blocks_actions_and_judgment()
-> crate::Result<()> {
    for (sequence, wrong_receipt) in [(2, false), (1, true)] {
        let mut native = receipts();
        native.inputs = vec![accepted(ITEM, sequence)];
        native.wrong_receipt = wrong_receipt;
        let script = Script::new([]);
        let output = execute(&script, &native, None).await?;
        assert!(output.task.is_none());
        assert!(output.recovery.is_some());
        assert!(script.sent().is_empty());
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
async fn uncertain_effect_is_read_by_original_identity_before_any_new_action() -> crate::Result<()>
{
    for state in [
        TaskEffectState::Prepared,
        TaskEffectState::Issued,
        TaskEffectState::Unknown,
    ] {
        let mut native = receipts();
        let confirmed = publication_fixture::Publisher::receipt();
        let mut previous = confirmed.clone();
        previous.state = state;
        previous.commit = None;
        native.effects = vec![previous];
        native.resolved = Some(confirmed);
        let script = Script::new([vec![say("Awaiting checks."), ended(false)]]);
        let output = execute(&script, &native, None).await?;
        assert_eq!(
            output
                .task
                .ok_or_else(crate::decision::unavailable)?
                .effect_watermark,
            1
        );
        assert_eq!(
            *native.reads.lock().expect("fixture mutex is unpoisoned"),
            vec![ITEM.to_owned()]
        );
        assert!(script.sent().first().is_some_and(|sent| sent.messages.iter().any(|message| matches!(message,af_providers::Message::User(text) if text.contains("native_publication_receipt")))));
    }
    Ok(())
}

#[tokio::test]
async fn unknown_or_mismatched_effect_receipt_blocks_actions_and_completion() -> crate::Result<()> {
    for mutation in ["unknown", "identity", "ordinal", "head", "tree", "commit"] {
        let mut native = receipts();
        let mut previous = publication_fixture::Publisher::receipt();
        previous.state = TaskEffectState::Unknown;
        previous.commit = None;
        let mut resolved = publication_fixture::Publisher::receipt();
        match mutation {
            "unknown" => {
                resolved.state = TaskEffectState::Unknown;
                resolved.commit = None;
            }
            "identity" => resolved.effect_id = PENDING.to_owned(),
            "ordinal" => resolved.ordinal = 2,
            "head" => resolved.expected_head = "e".repeat(40),
            "tree" => resolved.tested_tree = "e".repeat(40),
            _ => resolved.commit = None,
        }
        native.effects = vec![previous];
        native.resolved = Some(resolved);
        let script = Script::new([]);
        let output = execute(&script, &native, None).await?;
        assert!(output.task.is_none());
        assert!(output.recovery.is_some());
        assert!(script.sent().is_empty());
        assert!(
            native
                .goals
                .requests
                .lock()
                .expect("fixture mutex is unpoisoned")
                .is_empty()
        );
        assert_eq!(
            *native.reads.lock().expect("fixture mutex is unpoisoned"),
            vec![ITEM.to_owned()]
        );
    }
    Ok(())
}

#[tokio::test]
async fn replacement_assesses_authenticated_history_before_another_model_action()
-> crate::Result<()> {
    let original = receipts();
    let first_script = Script::new([vec![
        say("The failure is isolated; external checks are pending."),
        ended(false),
    ]]);
    let first = execute(&first_script, &original, None).await?;
    let recovered = first.recovery.ok_or_else(crate::decision::unavailable)?;
    let script = Script::new([]);
    let native = receipts();
    let output = execute(&script, &native, Some(&recovered)).await?;
    assert!(script.sent().is_empty());
    assert_eq!(
        output
            .task
            .ok_or_else(crate::decision::unavailable)?
            .disposition,
        TaskDisposition::Waiting
    );
    assert_eq!(
        output.result.content,
        "Recovery reconciled; no new action has run."
    );
    let decisions = native
        .goals
        .requests
        .lock()
        .expect("fixture mutex is unpoisoned");
    assert_eq!(decisions.len(), 1);
    assert!(
        decisions
            .first()
            .ok_or_else(crate::decision::unavailable)?
            .evidence
            .to_string()
            .contains("The failure is isolated; external checks are pending.")
    );
    assert!(output.recovery.is_some());
    Ok(())
}
