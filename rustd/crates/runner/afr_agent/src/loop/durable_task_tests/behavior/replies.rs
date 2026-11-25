//! Ordered source reply outcomes through the public root execution boundary.

#![expect(
    clippy::expect_used,
    reason = "test fixture: poisoned state must fail the test"
)]

use super::replies_fixture::*;
use super::*;

#[tokio::test]
async fn answered_items_and_inputs_not_yet_incorporated_get_no_new_reply() -> crate::Result<()> {
    let mut later = item(PENDING, DurableTaskInputAction::Steer, true);
    later.binding.input_sequence = Some(5);
    let mut queued = item(SOURCE, DurableTaskInputAction::Steer, false);
    queued.response_text = Some("Earlier answer.".to_owned());
    let native = source(vec![
        later,
        item(ITEM, DurableTaskInputAction::NewWork, true),
        queued,
    ]);
    let script = Script::new([vec![say("Work needs checks."), ended(false)]]);

    let output = execute(&script, &native).await?;

    // Both active items share the one task answer; the earlier reply stands.
    assert_eq!(
        output.result.content,
        "Work needs checks.\n\nEarlier answer."
    );
    let receipts = native.receipts.lock().expect("fixture mutex is unpoisoned");
    assert_eq!(
        receipts
            .iter()
            .map(|receipt| receipt.item_id.as_str())
            .collect::<Vec<_>>(),
        vec![ITEM],
        "the input at sequence 5 is not incorporated, and the queued item was answered"
    );
    assert_eq!(script.sent().len(), 1);
    Ok(())
}
