//! The claim says whether a later message waits in the head's conversation
//! (M245 §8), so a run whose lease says none skips the ask before its first
//! model call; a message that lands after the claim folds on the ask after
//! the answer.

use afd_admission::{Producer, Reply};
use afd_connector::Provider;
use afd_fleet::lease::Verdict;

use super::support::{
    Checked, STEER_JSON, STEER_TEXT, claim_as_holder, report_read_through, send, send_to,
    workspace_of,
};
use crate::report_seed::{Held, held};

/// A Slack thread the held run did not start in: another conversation.
const OTHER_THREAD: &str =
    r#"{"team_id":"T024BE7LD","channel_id":"C0123456789","thread_ts":"1700000000.000200"}"#;
/// A Slack mention's payload.
const MENTION_JSON: &str = r#"{"human_message":"reset my password"}"#;

/// Whether a fresh claim of the held fleet's head says a message waits: the
/// holder lapses and the spare runner claims the same head again.
async fn reclaimed_says_waiting(held: &Held) -> Checked<bool> {
    held.fixtures
        .lapse_holder(&held.issued.lease_id, &held.fleet)
        .await;
    let reclaimed = crate::seed::select_fleet(&held.leases, &held.spare, &held.fleet)
        .await
        .ok_or("the spare reclaims the lapsed head")?;
    assert_eq!(reclaimed.event_id, held.event_id, "the same head");
    Ok(reclaimed.opened.inputs_waiting)
}

// Value: protects=the claim reads its head's conversation for a later
// message; fails_when=it answers true for a lone head, false behind a second
// message, or true for a message in another conversation; why_new=the first
// fold ask moved into the claim; seam=none; no_claim=whether the later
// message is foldable, which the claim does not judge.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn claim_reports_waiting_input() -> Checked {
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
        reclaimed_says_waiting(&followed).await?,
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
        !reclaimed_says_waiting(&elsewhere).await?,
        "another conversation's message does not wait for this run"
    );
    elsewhere.fixtures.cleanup().await;
    Ok(())
}

// Value: protects=a message admitted after a claim that found nothing is
// still claimed and settled by the run; fails_when=a lease saying false makes
// the claim after the answer skip too, and the message settles unread;
// why_new=the run now skips its first ask on false; seam=none; no_claim=the
// second model call, which the runner's fold tests prove.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn message_after_claim_folds_after_the_answer() -> Checked {
    let held = held().await;
    assert!(
        !held.acquired.opened.inputs_waiting,
        "issued with nothing waiting"
    );
    let workspace = workspace_of(&held).await?;
    let late = send(&held, &workspace, Producer::Steer, STEER_JSON).await?;

    let after_answer = claim_as_holder(&held, 0).await?;
    let folds = report_read_through(&held, late.sequence, Verdict::Succeeded).await?;

    assert_eq!(after_answer, [late.claimed(STEER_TEXT)]);
    assert_eq!(
        folds.closed.len(),
        1,
        "the late message settles read: {folds:?}"
    );
    held.fixtures.cleanup().await;
    Ok(())
}
