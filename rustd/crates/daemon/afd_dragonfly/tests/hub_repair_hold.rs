//! A subscribe that arrives while a node is being repaired, and a repair with
//! nothing to repair.
//!
//! While a primary's socket is down the driver sends a slot's commands to a
//! random node, and a replica confirms an `SSUBSCRIBE` it will never deliver
//! to. So a subscribe taken up mid-repair is held until the owner answers as
//! itself, then sent. The fake is one node, where a subscribe that went out
//! early WOULD be delivered to — which is what makes a quiet reader during the
//! hold the proof that nothing was sent.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::time::Duration;

use afd_dragonfly::Subscription;
use afd_dragonfly::test_util::NODE_REPAIR_WINDOW;
use tokio::time::Instant;

use crate::fake_redis::{FakeRedis, Reply};
use crate::hub_gap_faults::{BUDGET, FIRST, RETRY, SECOND, deliver, fake_and_hub, gap_on};

/// The id the slot map names as every slot's owner.
const OWNER: &str = "owner";

/// `CLUSTER MYID` answered by some node other than the owner.
const NOT_THE_OWNER: &str = "+someone-else\r\n";

/// `CLUSTER MYID` answered by the owner.
const THE_OWNER: &str = "+owner\r\n";

/// How long a held reader is watched for a frame it must not get.
const HOLD: Duration = Duration::from_millis(300);

/// A slot map at the fake's own address naming [`OWNER`] for every slot.
fn owned_by_owner(fake: &FakeRedis) -> &'static str {
    let port = fake
        .url()
        .rsplit(':')
        .next()
        .expect("the url names a port")
        .to_owned();
    let map = format!(
        "*1\r\n*3\r\n:0\r\n:16383\r\n*3\r\n$0\r\n\r\n:{port}\r\n${}\r\n{OWNER}\r\n",
        OWNER.len()
    );
    Box::leak(map.into_boxed_str())
}

/// How many times the fake was sent `command`.
fn sent(fake: &FakeRedis, command: &str) -> usize {
    fake.seen().iter().filter(|name| *name == command).count()
}

/// Waits for `check`, naming `what` if the budget runs out.
async fn until(what: &str, mut check: impl FnMut() -> bool) {
    let deadline = Instant::now() + BUDGET;
    while !check() {
        assert!(Instant::now() < deadline, "{what} did not happen");
        tokio::time::sleep(RETRY).await;
    }
}

/// Publishes on `reader`'s channel and proves nothing reaches it: no
/// subscription for it exists on the server yet.
async fn held(fake: &FakeRedis, reader: &mut Subscription) {
    fake.publish(reader.channel(), "too-early");
    let waiting = tokio::time::timeout(HOLD, reader.recv()).await;
    assert!(
        waiting.is_err(),
        "the subscribe went out early: {waiting:?}"
    );
}

/// Cuts the fake while its owner answers as someone else, and waits until the
/// repair is asking.
async fn cut_with_the_owner_away(fake: &FakeRedis) {
    fake.set_reply("CLUSTER SLOTS", Reply::Raw(owned_by_owner(fake)));
    fake.set_reply("CLUSTER MYID", Reply::Raw(NOT_THE_OWNER));
    let asked = sent(fake, "CLUSTER");
    fake.cut();
    until("the repair to ask who answers, twice", || {
        sent(fake, "CLUSTER") > asked + 2
    })
    .await;
}

/// A hub holding nothing still waits for the owner before it serves a
/// subscribe: the push does not say which node was lost, so a channel taken
/// up mid-repair could be the lost node's.
#[tokio::test(flavor = "multi_thread")]
async fn a_subscribe_taken_up_mid_repair_waits_for_its_owner() {
    let (fake, hub) = fake_and_hub().await;
    cut_with_the_owner_away(&fake).await;

    let mut late = hub.subscribe(SECOND);
    held(&fake, &mut late).await;

    fake.set_reply("CLUSTER MYID", Reply::Raw(THE_OWNER));
    deliver(&fake, SECOND, "after", &mut late).await;
    assert_eq!(hub.connections_opened(), 1, "a repair is not a redial");
}

/// A held channel and one taken up mid-repair both come back once the owner
/// answers: the first told about its gap, the second delivered to.
#[tokio::test(flavor = "multi_thread")]
async fn a_held_channel_and_a_late_one_both_resume_on_the_owner() {
    let (fake, hub) = fake_and_hub().await;
    let mut first = hub.subscribe(FIRST);
    deliver(&fake, FIRST, "primed", &mut first).await;
    cut_with_the_owner_away(&fake).await;

    let mut late = hub.subscribe(SECOND);
    held(&fake, &mut late).await;

    fake.set_reply("CLUSTER MYID", Reply::Raw(THE_OWNER));
    gap_on(&mut first).await;
    deliver(&fake, FIRST, "resumed", &mut first).await;
    deliver(&fake, SECOND, "after", &mut late).await;
    assert_eq!(hub.connections_opened(), 1, "a repair is not a redial");
}

/// A socket lost while the hub holds nothing is mended by the driver, and the
/// repair window it would once have waited out is not a redial.
#[tokio::test(flavor = "multi_thread")]
async fn a_blip_with_nothing_held_is_not_redialled() {
    let (fake, hub) = fake_and_hub().await;
    let cut = Instant::now();
    fake.cut();
    tokio::time::sleep_until(cut + NODE_REPAIR_WINDOW + Duration::from_millis(500)).await;
    assert_eq!(
        hub.connections_opened(),
        1,
        "nothing to repair, nothing redialled"
    );

    let mut after = hub.subscribe(FIRST);
    deliver(&fake, FIRST, "after", &mut after).await;
}
