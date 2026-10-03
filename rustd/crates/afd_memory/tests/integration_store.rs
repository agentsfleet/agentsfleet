//! Dimension 1.1 and 3.3: the Postgres store, reached only through the trait,
//! keeps today's window, upsert, sweep and eviction, and skips a share from a
//! fleet that may not publish.
#![expect(
    clippy::expect_used,
    reason = "integration test: an unmet precondition should fail the test loudly"
)]

use afd_core::clock::UnixMillis;
use afd_memory::window::DAILY_RETENTION_MS;
use afd_wire::memory::{MAX_ENTRIES_PER_FLEET, PINNED_CATEGORY, Visibility};

use crate::workspace::{Grants, Workspace, delta};

/// The instant the first capture lands at.
const START_AT: i64 = 1_760_000_000_000;
/// The scratch category a capture's sweep expires.
const DAILY: &str = "daily";
/// A category that is neither pinned nor swept.
const CONVERSATION: &str = "conversation";

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_postgres_store_keeps_fleet_memory_behaviour() {
    let space = Workspace::create().await;
    let fleet = space.fleet("keeper", Grants::default()).await;
    let memories = &space.memories;
    let at = |millis: i64| UnixMillis::from_millis(millis);

    // Upsert: three keys, then one of them again, holds three entries.
    let first = [
        delta("pinned", PINNED_CATEGORY, Visibility::Fleet),
        delta("chat", CONVERSATION, Visibility::Fleet),
        delta("scratch", DAILY, Visibility::Fleet),
    ];
    let stored = memories
        .capture(&fleet, &first, at(START_AT))
        .await
        .expect("a capture");
    assert_eq!((stored.stored, stored.skipped, stored.swept), (3, 0, 0));
    let again = [delta("chat", CONVERSATION, Visibility::Fleet)];
    memories
        .capture(&fleet, &again, at(START_AT + 1))
        .await
        .expect("a re-capture");
    let hydrated = memories.hydrate(&fleet).await.expect("a hydrate");
    let keys: Vec<_> = hydrated
        .memory
        .iter()
        .map(|entry| entry.key.as_ref())
        .collect();
    // `pinned` and `scratch` were captured in one call at one instant, so
    // their order falls to two UUIDv7 ids minted in the same millisecond,
    // which is their random bits: only what the re-capture moved is pinned.
    assert_eq!(
        keys.first(),
        Some(&"chat"),
        "newest first, a re-capture moved chat up: {keys:?}"
    );
    let mut rest = keys.get(1..).unwrap_or_default().to_vec();
    rest.sort_unstable();
    assert_eq!(rest, ["pinned", "scratch"], "{keys:?}");
    assert!(
        hydrated.shared.is_empty() && !hydrated.publish,
        "no grant, no change"
    );

    // Sweep: a capture past the retention removes the scratch note.
    let later = at(START_AT + DAILY_RETENTION_MS + 2);
    let swept = memories
        .capture(&fleet, &[], later)
        .await
        .expect("an empty capture");
    assert_eq!(swept.swept, 1, "the expired daily entry is swept");

    // Eviction: past the cap, the coldest non-core rows go and core stays.
    let filler: Vec<_> = (0..MAX_ENTRIES_PER_FLEET)
        .map(|at| delta(&format!("f{at:04}"), CONVERSATION, Visibility::Fleet))
        .collect();
    let capped = memories
        .capture(&fleet, &filler, at(START_AT + DAILY_RETENTION_MS + 3))
        .await
        .expect("a capture to the cap");
    assert_eq!(
        capped.evicted, 2,
        "two rows past the cap of {MAX_ENTRIES_PER_FLEET}"
    );
    let window = memories.hydrate(&fleet).await.expect("a hydrate");
    assert!(
        window.memory.iter().any(|entry| entry.key == "pinned"),
        "the pinned entry survives eviction"
    );
    assert!(
        window.memory.iter().all(|entry| entry.key != "chat"),
        "the oldest non-core entry was evicted"
    );
    space.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_push_skips_an_unpublished_share() {
    let space = Workspace::create().await;
    let fleet = space.fleet("private", Grants::default()).await;

    let pushed = [
        delta("deploy_target", PINNED_CATEGORY, Visibility::Workspace),
        delta("own", PINNED_CATEGORY, Visibility::Fleet),
    ];
    let captured = space
        .memories
        .capture(&fleet, &pushed, UnixMillis::from_millis(START_AT))
        .await
        .expect("a capture");

    assert_eq!((captured.stored, captured.skipped), (1, 1));
    assert_eq!(captured.unpublished, 1, "the skip is the share");
    let hydrated = space.memories.hydrate(&fleet).await.expect("a hydrate");
    let keys: Vec<_> = hydrated
        .memory
        .iter()
        .map(|entry| entry.key.as_ref())
        .collect();
    assert_eq!(keys, ["own"]);
    space.cleanup().await;
}
