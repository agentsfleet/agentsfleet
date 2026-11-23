//! The assignment says whether a later message waits in the head's conversation
//!, so a run whose run says none skips the ask before its first
//! model call; a message that lands after the assignment folds on the ask after
//! the answer.

use afd_connector::Provider;
use afd_fleet::run::Verdict;
use afd_turns::{Producer, Reply};

use super::support::{
    Checked, STEER_JSON, STEER_TEXT, assign_as_holder, report_read_through, send, send_to,
    workspace_of,
};
use crate::report_seed::{Held, held};

/// A Slack thread the held run did not start in: another conversation.
const OTHER_THREAD: &str =
    r#"{"team_id":"T024BE7LD","channel_id":"C0123456789","thread_ts":"1700000000.000200"}"#;
/// A Slack mention's payload.
const MENTION_JSON: &str = r#"{"human_message":"reset my password"}"#;

/// Whether a fresh assignment of the held fleet's head says a message waits: the
/// holder lapses and the spare runner is assigned the same head again.
async fn reassigned_says_waiting(held: &Held) -> Checked<bool> {
    held.fixtures
        .lapse_holder(&held.issued.run_id, &held.fleet)
        .await;
    let reassigned = crate::seed::select_fleet(&held.runs, &held.spare, &held.fleet)
        .await
        .ok_or("the spare is reassigned the lapsed head")?;
    assert_eq!(reassigned.turn_id, held.turn_id, "the same head");
    Ok(reassigned.opened.inputs_waiting)
}

// Value: protects=the assignment reads its head's conversation for a later
// message; fails_when=it answers true for a lone head, false behind a second
// message, or true for a message in another conversation; why_new=the first
// fold ask moved into the assignment; seam=none; no_claim=whether the later
// message is foldable, which the assignment does not judge.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn assignment_reports_waiting_input() -> Checked {
    let lone = held().await;
    assert!(
        !lone.acquired.opened.inputs_waiting,
        "a lone message has nothing after it"
    );
    lone.fixtures.cleanup().await;

    let followed = held().await;
    let workspace = workspace_of(&followed).await?;
    send(&followed, &workspace, Producer::Steer, STEER_JSON).await?;
    assert!(
        reassigned_says_waiting(&followed).await?,
        "the first of two in one conversation"
    );
    followed.fixtures.cleanup().await;

    let elsewhere = held().await;
    let workspace = workspace_of(&elsewhere).await?;
    let thread = Reply::To {
        connector: Provider::Slack.id(),
        address: OTHER_THREAD,
    };
    send_to(
        &elsewhere,
        &workspace,
        Producer::SlackMention,
        MENTION_JSON,
        thread,
    )
    .await?;
    assert!(
        !reassigned_says_waiting(&elsewhere).await?,
        "another conversation's message does not wait for this run"
    );
    elsewhere.fixtures.cleanup().await;
    Ok(())
}

// Value: protects=a message admitted after an assignment that found nothing is
// still assigned and settled by the run; fails_when=a run saying false makes
// the assignment after the answer skip too, and the message settles unread;
// why_new=the run now skips its first ask on false; seam=none; no_claim=the
// second model call, which the runner's fold tests prove.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn message_after_assignment_folds_after_the_answer() -> Checked {
    let held = held().await;
    assert!(
        !held.acquired.opened.inputs_waiting,
        "issued with nothing waiting"
    );
    let workspace = workspace_of(&held).await?;
    let late = send(&held, &workspace, Producer::Steer, STEER_JSON).await?;

    let after_answer = assign_as_holder(&held, 0).await?;
    let folds = report_read_through(&held, late.sequence, Verdict::Succeeded).await?;

    assert_eq!(after_answer, [late.assigned(STEER_TEXT)]);
    assert_eq!(
        folds.closed.len(),
        1,
        "the late message settles read: {folds:?}"
    );
    held.fixtures.cleanup().await;
    Ok(())
}
