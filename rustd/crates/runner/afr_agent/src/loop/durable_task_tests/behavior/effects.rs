//! The native publication page is walked in ordinal order, and the task's
//! cursors must agree with what the walk reconciled before the root acts.

use super::recovery_fixture::*;
use super::*;

fn refused_effect(ordinal: u64) -> DurableTaskEffect {
    let mut effect = publication_fixture::Publisher::receipt();
    effect.ordinal = ordinal;
    effect.state = DurableTaskEffectState::Refused;
    effect.commit = None;
    effect
}

#[tokio::test]
async fn a_refused_publication_is_reconciled_without_raising_the_confirmed_watermark()
-> crate::Result<()> {
    let mut native = receipts();
    native.effects = vec![refused_effect(1)];
    let script = Script::new([vec![say("Awaiting checks."), ended(false)]]);

    let output = execute(&script, &native, None).await?;

    let report = output.task.ok_or_else(crate::decision::unavailable)?;
    assert_eq!(report.effect_watermark, 0);
    assert!(
        native.reads.lock().is_ok_and(|reads| reads.is_empty()),
        "a settled refusal is not read back"
    );
    assert!(script.sent().first().is_some_and(|sent| sent.messages.iter().any(
        |message| matches!(message, guffbolt::Message::User(text) if text.contains("native_publication_receipt"))
    )));
    Ok(())
}

#[tokio::test]
async fn a_gap_in_publication_ordinals_or_a_disagreeing_task_cursor_blocks_every_action()
-> crate::Result<()> {
    for case in ["gap", "watermark", "endless"] {
        let mut native = receipts();
        match case {
            "gap" => native.effects = vec![refused_effect(2)],
            "watermark" => {
                // The native task claims a confirmation the page never showed.
                native.effects = vec![refused_effect(1)];
                let mut confirmed = publication_fixture::Publisher::receipt();
                confirmed.ordinal = 7;
                native.resolved = Some(confirmed);
            }
            _ => native.endless = true,
        }
        let script = Script::new([]);

        let output = execute(&script, &native, None).await?;

        assert!(script.sent().is_empty(), "{case}: the model was called");
        assert!(output.task.is_none(), "{case}");
        assert!(
            native
                .goals
                .requests
                .lock()
                .is_ok_and(|requests| requests.is_empty()),
            "{case}: a goal was assessed"
        );
    }
    Ok(())
}
