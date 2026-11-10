//! The root folds what its fleet receives while it works: a message sent
//! before or during the run reaches the next model call, the claim advances
//! past what was read, and only an answered run says how far it read.

mod fixture;

use std::sync::atomic::AtomicUsize;

use af_wire::report::FailureClass;
use afr_tools::catalog::HTTP_REQUEST;
use tokio_util::sync::CancellationToken;

use self::fixture::{Arriving, Interrupted, Replaying, lost, run_with, shape, users};
use super::finish::DETAIL_STOPPED;
use crate::fixture::{Script, call, lease, say, unbounded};
use crate::{Failure, Folded, ResultOutcome};

#[tokio::test]
async fn a_message_waiting_when_the_run_starts_joins_its_opening_turn() -> crate::Result<()> {
    let script = Script::new([vec![say("Deployed to staging.")]]);
    let payload = lease(&[HTTP_REQUEST.name()], unbounded());
    let inbox = Arriving::default();
    inbox.on_claim(0, 7, "actually, use staging");
    let output = run_with(&script, &payload, &inbox, &CancellationToken::new()).await?;
    let opening = users(&script, 0);
    assert_eq!(
        opening.len(),
        1,
        "one user turn, never two in a row: {opening:?}"
    );
    assert!(
        opening
            .first()
            .is_some_and(|said| said.ends_with("actually, use staging")),
        "{opening:?}"
    );
    assert_eq!(output.folded_through_sequence, Some(7));
    Ok(())
}

/// The claims one answering run asks for, under a lease whose
/// `inputs_waiting` is `flag`, or absent when `None`.
async fn answered(flag: Option<bool>) -> crate::Result<Vec<i64>> {
    let script = Script::new([vec![say("Deployed to staging.")]]);
    let mut payload = lease(&[HTTP_REQUEST.name()], unbounded());
    if let Some(waiting) = flag {
        payload.inputs_waiting = waiting;
    }
    let inbox = Arriving::default();
    run_with(&script, &payload, &inbox, &CancellationToken::new()).await?;
    Ok(inbox.asked())
}

// Value: protects=a lease whose claim found nothing waiting skips the ask
// before its first model call, and asks after the answer; fails_when=the skip
// never happens, swallows the ask after the answer, or fires on a lease that
// said true or nothing; why_new=M245 §8 moved that ask into the claim;
// seam=none; no_claim=what the daemon computes, which its claim test proves.
#[tokio::test]
async fn first_model_call_skips_the_answered_fold_ask() -> crate::Result<()> {
    assert_eq!(
        answered(Some(false)).await?,
        [0],
        "one ask, after the answer"
    );
    assert_eq!(answered(Some(true)).await?, [0, 0], "before and after");
    assert_eq!(
        answered(None).await?,
        [0, 0],
        "an absent flag reads as true"
    );
    Ok(())
}

#[tokio::test]
async fn a_message_sent_while_tools_run_is_read_by_the_next_call() -> crate::Result<()> {
    let script = Script::new([
        vec![call("c1", HTTP_REQUEST.name(), serde_json::json!({}))],
        vec![say("Deployed, and tagged.")],
    ]);
    let payload = lease(&[HTTP_REQUEST.name()], unbounded());
    let inbox = Arriving::default();
    inbox.on_claim(1, 9, "and tag it");
    let output = run_with(&script, &payload, &inbox, &CancellationToken::new()).await?;
    assert!(
        !users(&script, 0).iter().any(|said| said.contains("tag it")),
        "the first call had its request already"
    );
    assert!(
        users(&script, 1)
            .iter()
            .any(|said| said.contains("and tag it"))
    );
    assert_eq!(output.folded_through_sequence, Some(9));
    assert_eq!(
        inbox.asked(),
        [0, 0, 9],
        "nothing was read before claim 1, and the claim after the answer asks past it"
    );
    Ok(())
}

#[tokio::test]
async fn a_message_sent_while_the_answer_is_written_gets_one_more_turn() -> crate::Result<()> {
    let script = Script::new([
        vec![say("Deployed to prod.")],
        vec![say("Moved to staging.")],
    ]);
    let payload = lease(&[HTTP_REQUEST.name()], unbounded());
    let inbox = Arriving::default();
    inbox.on_claim(1, 11, "wait, staging not prod");
    let output = run_with(&script, &payload, &inbox, &CancellationToken::new()).await?;
    assert_eq!(
        script.sent().len(),
        2,
        "the answer was read by one more call"
    );
    assert!(matches!(output.result.outcome, ResultOutcome::Completed(_)));
    assert_eq!(
        output.result.content,
        ["Deployed to prod.", "Moved to staging."].join(guffbolt::ANSWER_JOIN)
    );
    assert_eq!(output.folded_through_sequence, Some(11));
    Ok(())
}

#[tokio::test]
async fn a_correction_sent_while_the_answer_is_written_follows_that_answer() -> crate::Result<()> {
    let script = Script::new([
        vec![say("Deployed to prod.")],
        vec![say("Moved to staging.")],
    ]);
    let payload = lease(&[HTTP_REQUEST.name()], unbounded());
    let inbox = Arriving::default();
    inbox.on_claim(1, 11, "wait, staging not prod");
    run_with(&script, &payload, &inbox, &CancellationToken::new()).await?;
    let expected: Vec<_> = shape(&script, 0)
        .into_iter()
        .chain([
            ("assistant", "Deployed to prod.".to_owned()),
            ("user", "wait, staging not prod".to_owned()),
        ])
        .collect();
    assert_eq!(
        shape(&script, 1),
        expected,
        "the request ends with the correction, and the opening is not edited"
    );
    Ok(())
}

#[tokio::test]
async fn a_correction_after_tool_calls_follows_the_answer_it_interrupted() -> crate::Result<()> {
    let script = Script::new([
        vec![call("c1", HTTP_REQUEST.name(), serde_json::json!({}))],
        vec![say("Deployed to prod.")],
        vec![say("Moved to staging.")],
    ]);
    let payload = lease(&[HTTP_REQUEST.name()], unbounded());
    let inbox = Arriving::default();
    inbox.on_claim(2, 12, "wait, staging not prod");
    run_with(&script, &payload, &inbox, &CancellationToken::new()).await?;
    let expected: Vec<_> = shape(&script, 1)
        .into_iter()
        .chain([
            ("assistant", "Deployed to prod.".to_owned()),
            ("user", "wait, staging not prod".to_owned()),
        ])
        .collect();
    assert_eq!(
        shape(&script, 2),
        expected,
        "the request ends with the correction, after the tool results and the answer"
    );
    Ok(())
}

#[tokio::test]
async fn a_blank_answer_after_a_correction_adds_nothing_to_the_reply() -> crate::Result<()> {
    let script = Script::new([vec![say("Deployed to prod.")], vec![say("  ")]]);
    let payload = lease(&[HTTP_REQUEST.name()], unbounded());
    let inbox = Arriving::default();
    inbox.on_claim(1, 11, "wait, staging not prod");
    let output = run_with(&script, &payload, &inbox, &CancellationToken::new()).await?;
    assert_eq!(output.result.content, "Deployed to prod.");
    Ok(())
}

#[tokio::test]
async fn the_claim_moves_past_what_the_run_read() -> crate::Result<()> {
    let script = Script::new([
        vec![call("c1", HTTP_REQUEST.name(), serde_json::json!({}))],
        vec![say("Done.")],
    ]);
    let payload = lease(&[HTTP_REQUEST.name()], unbounded());
    let inbox = Arriving::default();
    inbox.on_claim(0, 3, "first");
    run_with(&script, &payload, &inbox, &CancellationToken::new()).await?;
    assert_eq!(
        inbox.asked(),
        [0, 3, 3],
        "every claim after the read asks past the message already read"
    );
    Ok(())
}

#[tokio::test]
async fn a_message_the_inbox_replays_is_joined_once() -> crate::Result<()> {
    let script = Script::new([
        vec![call("c1", HTTP_REQUEST.name(), serde_json::json!({}))],
        vec![say("Done.")],
    ]);
    let payload = lease(&[HTTP_REQUEST.name()], unbounded());
    let inbox = Replaying {
        message: Folded {
            sequence: 4,
            text: "only once".to_owned(),
        },
        replays: 2,
        claims: AtomicUsize::new(0),
    };
    let output = run_with(&script, &payload, &inbox, &CancellationToken::new()).await?;
    let joined: usize = users(&script, 1)
        .iter()
        .map(|said| said.matches("only once").count())
        .sum();
    assert_eq!(joined, 1);
    assert_eq!(output.folded_through_sequence, Some(4));
    Ok(())
}

#[tokio::test]
async fn a_stop_during_the_claim_joins_nothing() -> crate::Result<()> {
    let script = Script::new([vec![say("unreachable")]]);
    let payload = lease(&[HTTP_REQUEST.name()], unbounded());
    let stop = CancellationToken::new();
    let inbox = Interrupted { stop: stop.clone() };
    let output = run_with(&script, &payload, &inbox, &stop).await?;
    assert!(
        !users(&script, 0)
            .iter()
            .any(|said| said.contains("too late")),
        "a claim the stop overtook hands the run nothing"
    );
    assert_eq!(
        output.result.outcome,
        ResultOutcome::Failed(Failure {
            class: None,
            detail: DETAIL_STOPPED.into(),
        })
    );
    assert_eq!(output.folded_through_sequence, None);
    Ok(())
}

#[tokio::test]
async fn a_run_that_folded_nothing_reports_no_cursor() -> crate::Result<()> {
    let script = Script::new([vec![say("Done.")]]);
    let payload = lease(&[HTTP_REQUEST.name()], unbounded());
    let output = run_with(
        &script,
        &payload,
        &Arriving::default(),
        &CancellationToken::new(),
    )
    .await?;
    assert_eq!(output.folded_through_sequence, None);
    Ok(())
}

#[tokio::test]
async fn a_failed_run_reports_no_cursor_so_its_folds_run_again() -> crate::Result<()> {
    let script = Script::failing(vec![], lost);
    let payload = lease(&[HTTP_REQUEST.name()], unbounded());
    let inbox = Arriving::default();
    inbox.on_claim(0, 5, "are you there?");
    let output = run_with(&script, &payload, &inbox, &CancellationToken::new()).await?;
    assert_eq!(
        output.result.outcome,
        ResultOutcome::Failed(Failure {
            class: Some(FailureClass::TransportLoss),
            detail: "the connection to the model provider was lost mid-turn".into(),
        })
    );
    assert_eq!(output.folded_through_sequence, None);
    Ok(())
}
