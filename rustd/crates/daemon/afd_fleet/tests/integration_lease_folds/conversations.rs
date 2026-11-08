//! A run folds only its own conversation: a message from another thread is
//! never claimed, never in the way, and never sent this run's answer.

use af_core::id::Uuid7;
use afd_admission::{Producer, Reply};
use afd_connector::Provider;
use afd_fleet::lease::Verdict;

use super::support::{
    Checked, STEER_JSON, STEER_TEXT, Settlement, address_head, claim_as_holder, link_slack_person,
    report_read_through, send, send_as, send_to, settled, workspace_of,
};
use crate::report_seed::held;

/// The Slack thread a Slack-started run answers in.
const THREAD_A: &str =
    r#"{"team_id":"T0FOLDTEAM","channel_id":"C0123456789","thread_ts":"1700000000.000100"}"#;
/// A Slack thread the held run did not start in.
const OTHER_THREAD: &str =
    r#"{"team_id":"T024BE7LD","channel_id":"C0123456789","thread_ts":"1700000000.000200"}"#;
/// A Slack mention's payload.
const MENTION_JSON: &str = r#"{"human_message":"reset my password"}"#;

/// The held run serves the fleet's unaddressed conversation. A mention in a
/// Slack thread is another conversation: the person's steer after it still
/// folds, the mention is left for a run of its own, and the report settles
/// and answers only what the run read.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_another_conversation_is_neither_claimed_nor_in_the_way() -> Checked {
    let held = held().await;
    let workspace = workspace_of(&held).await?;
    let thread = Reply::To {
        connector: Provider::Slack.id(),
        address: OTHER_THREAD,
    };
    let mention = send_to(
        &held,
        &workspace,
        Producer::SlackMention,
        MENTION_JSON,
        thread,
    )
    .await?;
    let steer = send(&held, &workspace, Producer::Steer, STEER_JSON).await?;
    assert!(!steer.run.earlier_waiting, "{:?}", steer.run);
    assert_eq!(
        claim_as_holder(&held, 0).await?,
        [steer.claimed(STEER_TEXT)]
    );
    let folds = report_read_through(&held, steer.sequence, Verdict::Succeeded).await?;
    assert_eq!(folds.closed.len(), 1, "{folds:?}");
    assert_eq!(
        settled(&held, &mention.id).await?,
        Settlement::UNREAD,
        "the other thread hears nothing from this run"
    );
    held.fixtures.cleanup().await;
    Ok(())
}

/// A Slack team and the people the tests below link or leave unlinked.
const TEAM: &str = "T0FOLDTEAM";
const LINKED: &str = "ULINKEDPERSON";
const STRANGER: &str = "USTRANGERONE";

/// A run that started in a Slack thread folds that thread's linked people and
/// nobody else: a stranger's mention in the thread is a message no verified
/// person sent, so the fold stops at it, and a mention in another thread is
/// neither claimed nor in the way.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_slack_thread_run_folds_its_threads_linked_people_only() -> Checked {
    let held = held().await;
    let workspace = workspace_of(&held).await?;
    address_head(&held, THREAD_A).await?;
    link_slack_person(&held, &workspace, TEAM, LINKED).await?;
    let thread = |address| Reply::To {
        connector: Provider::Slack.id(),
        address,
    };
    let actor = |person: &str| format!("slack:{TEAM}:{person}");
    let mention = Producer::SlackMention;
    let linked = actor(LINKED);
    let stranger = actor(STRANGER);
    let first = send_as(
        &held,
        &workspace,
        mention,
        &linked,
        MENTION_JSON,
        thread(THREAD_A),
    )
    .await?;
    let elsewhere = send_as(
        &held,
        &workspace,
        mention,
        &linked,
        MENTION_JSON,
        thread(OTHER_THREAD),
    )
    .await?;
    let second = send_as(
        &held,
        &workspace,
        mention,
        &linked,
        MENTION_JSON,
        thread(THREAD_A),
    )
    .await?;
    let unknown = send_as(
        &held,
        &workspace,
        mention,
        &stranger,
        MENTION_JSON,
        thread(THREAD_A),
    )
    .await?;
    let behind = send_as(
        &held,
        &workspace,
        mention,
        &linked,
        MENTION_JSON,
        thread(THREAD_A),
    )
    .await?;
    let claimed: Vec<Uuid7> = claim_as_holder(&held, 0)
        .await?
        .into_iter()
        .map(|message| message.id)
        .collect();
    assert_eq!(
        claimed,
        [first.id.clone(), second.id.clone()],
        "the thread's linked person folds up to the stranger's mention \
         ({} elsewhere, {} unknown, {} behind it)",
        elsewhere.id,
        unknown.id,
        behind.id
    );
    held.fixtures.cleanup().await;
    Ok(())
}
