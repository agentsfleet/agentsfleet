//! A primary that stays down while the doorbell holds a channel on it.
//!
//! Split from `hub_socket_faults.rs` per RULE FLL; it shares that file's fake,
//! budget and polling helper. The pacing itself, a generation that held
//! restarting the climb and a capped schedule starting again, is proven
//! without a socket in `hub/pump/pacing.rs`; this is the hub doing it.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::time::Duration;

use afd_dragonfly::config::{DragonflyConfig, DragonflyRole};
use afd_dragonfly::{Doorbell, ReadyPrefix, SubscriptionHub};
use backon::ExponentialBuilder;

use crate::fake_redis::{FakeRedis, Reply, install_subscriber};
use crate::hub_socket_faults::{BUDGET, pubsub_rules, until};

/// The climbing schedule's first step and ceiling: production's shape,
/// scaled down eightfold and without jitter so the count is steady.
const FIRST_STEP: Duration = Duration::from_millis(25);
const CEILING: Duration = Duration::from_millis(200);

/// A cluster that keeps refusing the resubscribes after each redial, a
/// primary down while the doorbell holds a channel on it, is redialled on a
/// climbing wait, not at the schedule's first step forever.
///
/// The doorbell holds its sixteen channels for the life of the process, so
/// every replica has channels on every primary. With the wait restarting each
/// lap, one dead primary would redial every replica's hub about forty times a
/// second here (32 measured), each lap re-confirming the healthy channels into a gap for
/// every reader. With the wait climbing to its ceiling the same second holds
/// a handful.
#[tokio::test(flavor = "multi_thread")]
async fn test_a_cluster_refusing_resubscribes_is_redialled_on_a_climbing_wait() {
    /// How long the redials are counted for once the refusals start.
    const WINDOW: Duration = Duration::from_secs(1);
    /// More than a climbing schedule fits in the window, far fewer than a flat
    /// first step does: 25 + 50 + 100 + 200 ms, then 200 ms each, passes a
    /// second in eight waits, where the flat first step fits forty (32 measured).
    const MOST_REDIALS: u64 = 12;

    install_subscriber();
    let server = FakeRedis::spawn(&pubsub_rules()).await;
    let config = DragonflyConfig::from_url(DragonflyRole::Default, server.url());
    let climbing = ExponentialBuilder::new()
        .with_min_delay(FIRST_STEP)
        .with_max_delay(CEILING)
        .without_max_times();
    let hub = tokio::time::timeout(
        BUDGET,
        SubscriptionHub::start_with_backoff(config, climbing),
    )
    .await
    .expect("the fake serves, so the hub must start")
    .expect("the fake serves, so the hub must start");
    let doorbell = Doorbell::new();
    doorbell.listen(&hub, &ReadyPrefix::production());
    until("the doorbell's subscribes to reach the server", || {
        server.seen().iter().any(|command| command == "SSUBSCRIBE")
    })
    .await;

    server.set_reply("SSUBSCRIBE", Reply::Hangup);
    server.cut();
    until("the pump to redial onto the refusing server", || {
        hub.connections_opened() >= 2
    })
    .await;
    let before = hub.connections_opened();
    tokio::time::sleep(WINDOW).await;
    let redials = hub.connections_opened().saturating_sub(before);

    assert!(redials >= 1, "the pump keeps redialling: {redials}");
    assert!(
        redials <= MOST_REDIALS,
        "{redials} redials in {WINDOW:?}: the wait is not climbing"
    );
}
