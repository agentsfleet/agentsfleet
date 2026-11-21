//! The fence and the numbering hold for a run whose line has somewhere
//! to go.
//!
//! Every case runs an event admitted from a Slack thread, so a post that got
//! past the fence would land and be seen: a refusal here is proved by an empty
//! thread, never by the absence of a thread.
//!
//! Marked `#[ignore]` like the rest of the live-service suite; run by
//! `make test-integration-rustd`.
#![expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "test target: an unmet precondition should fail the test loudly, and a step \
              indexes the JSON it was answered"
)]

use std::collections::BTreeSet;

use agentsfleetd::supervisor::Supervisor;
use serde_json::json;

use crate::speaking::Speaking;
use crate::verbs::posts;

/// How many posts race: past the eight a run was once held to.
const RACERS: u32 = 12;

/// A token that is not the run's own posts nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_stale_fence_message_refused() {
    let mut supervisor = Supervisor::new();
    let speaking = Speaking::boot(&mut supervisor, crate::e2e_seed::FLEET_CONFIG_JSON).await;
    let (run_id, fence) = speaking.run_thread().await;

    let (status, refused) = speaking.say((&run_id, fence - 1), "late").await;
    assert_eq!(
        (status, &refused["error_code"]),
        (409, &json!("UZ-RUN-005")),
        "{refused}"
    );
    assert!(posts(&speaking.slack).is_empty(), "the thread was there");
    assert_eq!(speaking.counted(&run_id).await, 0);
    speaking.finish(supervisor).await;
}

/// The failure mode that matters: a holder a reassignment has superseded
/// presents its own token, and its line never reaches the thread.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_superseded_holder_of_a_thread_posts_nothing() {
    let mut supervisor = Supervisor::new();
    let speaking = Speaking::boot(&mut supervisor, crate::e2e_seed::FLEET_CONFIG_JSON).await;
    let (run_id, fence) = speaking.run_thread().await;
    speaking.supersede().await;

    let (status, refused) = speaking.say((&run_id, fence), "late").await;
    assert_eq!(
        (status, &refused["error_code"]),
        (409, &json!("UZ-RUN-005")),
        "{refused}"
    );
    assert!(posts(&speaking.slack).is_empty(), "the thread was there");
    assert_eq!(speaking.counted(&run_id).await, 0);
    speaking.finish(supervisor).await;
}

/// Posts racing for numbers: every one lands, no two take the same line, and
/// the count is exactly how many raced.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_concurrent_messages_each_take_their_own_line() {
    let mut supervisor = Supervisor::new();
    let speaking = Speaking::boot(&mut supervisor, crate::e2e_seed::FLEET_CONFIG_JSON).await;
    let (run_id, fence) = speaking.run_thread().await;
    let texts: Vec<String> = (0..RACERS).map(|line| format!("line {line}")).collect();

    let answers = futures_util::future::join_all(
        texts
            .iter()
            .map(|text| speaking.say((&run_id, fence), text)),
    )
    .await;
    let landed = answers.iter().filter(|(status, _)| *status == 200).count();
    let racers = usize::try_from(RACERS).expect("a small count");
    assert_eq!(landed, racers, "{answers:?}");
    let lines: BTreeSet<u64> = posts(&speaking.slack)
        .iter()
        .filter_map(|sent| sent["metadata"]["event_payload"]["part"]["line"].as_u64())
        .collect();
    let expected: BTreeSet<u64> = (1..=u64::from(RACERS)).collect();
    assert_eq!(lines, expected, "each post took its own line");
    let counted = i32::try_from(RACERS).expect("a small count");
    assert_eq!(speaking.counted(&run_id).await, counted);
    speaking.finish(supervisor).await;
}
