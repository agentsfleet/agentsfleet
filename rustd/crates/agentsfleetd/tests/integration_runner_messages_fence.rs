//! §2 — the fence and the cap hold for a lease whose line has somewhere to go.
//!
//! Every case leases an event admitted from a Slack thread, so a post that got
//! past the fence would land and be seen: a refusal here is proved by an empty
//! thread, never by the absence of a thread.
//!
//! Marked `#[ignore]` like the rest of the live-service suite; run by
//! `make test-integration-rustd`.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "test target: an unmet precondition should fail the test loudly, and a step \
              indexes the JSON it was answered"
)]

use afd_wire::message_verb::MESSAGES_PER_RUN_MAX;
use agentsfleetd::supervisor::Supervisor;
use serde_json::json;

use crate::speaking::Speaking;
use crate::verbs::posts;

/// How many posts race for the cap: half again past it.
const RACERS: u32 = MESSAGES_PER_RUN_MAX + MESSAGES_PER_RUN_MAX / 2;

/// Dimension 2.2. A token that is not the lease's own posts nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_stale_fence_message_refused() {
    let mut supervisor = Supervisor::new();
    let speaking = Speaking::boot(&mut supervisor, crate::e2e_seed::FLEET_CONFIG_JSON).await;
    let (lease_id, fence) = speaking.lease_thread().await;

    let (status, refused) = speaking.say((&lease_id, fence - 1), "late").await;
    assert_eq!(
        (status, &refused["error_code"]),
        (409, &json!("UZ-RUN-005")),
        "{refused}"
    );
    assert!(posts(&speaking.slack).is_empty(), "the thread was there");
    assert_eq!(speaking.counted(&lease_id).await, 0);
    speaking.finish(supervisor).await;
}

/// Dimension 2.2, the spec's failure mode. A holder a reclaim has superseded
/// presents its own token, and its line never reaches the thread.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_superseded_holder_of_a_thread_posts_nothing() {
    let mut supervisor = Supervisor::new();
    let speaking = Speaking::boot(&mut supervisor, crate::e2e_seed::FLEET_CONFIG_JSON).await;
    let (lease_id, fence) = speaking.lease_thread().await;
    speaking.supersede().await;

    let (status, refused) = speaking.say((&lease_id, fence), "late").await;
    assert_eq!(
        (status, &refused["error_code"]),
        (409, &json!("UZ-RUN-005")),
        "{refused}"
    );
    assert!(posts(&speaking.slack).is_empty(), "the thread was there");
    assert_eq!(speaking.counted(&lease_id).await, 0);
    speaking.finish(supervisor).await;
}

/// Posts racing for the last slots: exactly the cap land, every other is
/// refused, and the count never passes the cap.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_concurrent_messages_never_pass_the_cap() {
    let mut supervisor = Supervisor::new();
    let speaking = Speaking::boot(&mut supervisor, crate::e2e_seed::FLEET_CONFIG_JSON).await;
    let (lease_id, fence) = speaking.lease_thread().await;
    let texts: Vec<String> = (0..RACERS).map(|line| format!("line {line}")).collect();

    let answers = futures_util::future::join_all(
        texts
            .iter()
            .map(|text| speaking.say((&lease_id, fence), text)),
    )
    .await;
    let landed = answers.iter().filter(|(status, _)| *status == 200).count();
    let capped = answers
        .iter()
        .filter(|(status, body)| *status == 409 && body["error_code"] == "UZ-RUN-020")
        .count();
    let cap = usize::try_from(MESSAGES_PER_RUN_MAX).expect("a small cap");
    let racers = usize::try_from(RACERS).expect("a small count");
    assert_eq!((landed, capped), (cap, racers - cap), "{answers:?}");
    assert_eq!(posts(&speaking.slack).len(), cap);
    let counted = i32::try_from(MESSAGES_PER_RUN_MAX).expect("a small cap");
    assert_eq!(speaking.counted(&lease_id).await, counted);
    speaking.finish(supervisor).await;
}
