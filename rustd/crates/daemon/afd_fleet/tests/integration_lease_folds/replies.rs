//! The replies a report owes for the messages it folded.

use afd_admission::{Producer, Reply};
use afd_connector::Provider;
use afd_fleet::lease::Verdict;

use super::support::{Checked, claim_as_holder, report_read_through, send_to, touch, workspace_of};
use crate::report_seed::held;

/// A Slack thread, as its connector states a destination.
const THREAD_A: &str =
    r#"{"team_id":"T024BE7LD","channel_id":"C0123456789","thread_ts":"1700000000.000100"}"#;
/// Another thread in the same channel.
const THREAD_B: &str =
    r#"{"team_id":"T024BE7LD","channel_id":"C0123456789","thread_ts":"1700000000.000200"}"#;
/// A Slack mention's payload.
const MENTION_JSON: &str = r#"{"human_message":"is the deploy done?"}"#;

/// Two folded mentions from one thread owe that thread one reply, threaded
/// onto the older mention; a third from another thread owes its own.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_folds_from_one_thread_owe_it_one_reply_on_its_oldest_message() -> Checked {
    let held = held().await;
    let workspace = workspace_of(&held).await?;
    let thread = |address| Reply::To {
        connector: Provider::Slack.id(),
        address,
    };
    let mention = Producer::SlackMention;
    let oldest = send_to(&held, &workspace, mention, MENTION_JSON, thread(THREAD_A)).await?;
    let newer = send_to(&held, &workspace, mention, MENTION_JSON, thread(THREAD_A)).await?;
    let other = send_to(&held, &workspace, mention, MENTION_JSON, thread(THREAD_B)).await?;
    assert_eq!(claim_as_holder(&held, 0).await?.len(), 3);
    // The settle statement promises no row order. Giving the oldest mention
    // the newest row version is the order a scan with no sort may return it
    // in, so a handler that trusted that order would thread onto `newer`.
    touch(&held, &oldest.id).await?;
    let folds = report_read_through(&held, other.sequence, Verdict::Succeeded).await?;
    assert_eq!(folds.closed.len(), 3, "each mention's own event closes");
    let mut owed: Vec<(&str, &str)> = folds
        .owed
        .iter()
        .map(|(owing, event)| (owing.reply.address.as_str(), event.as_str()))
        .collect();
    owed.sort_unstable();
    assert_eq!(
        owed,
        [
            (THREAD_A, oldest.id.as_str()),
            (THREAD_B, other.id.as_str())
        ],
        "one reply per thread, on its oldest message (not {})",
        newer.id
    );
    held.fixtures.cleanup().await;
    Ok(())
}
