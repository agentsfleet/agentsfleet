//! What the hub does when a node repair, a redial, or the server's own pushes
//! go wrong.
//!
//! The fake is the service, so these run in the fast lane. Tests that read
//! the hub's log run on a current-thread runtime: the recorder is thread-local,
//! and there the hub's tasks share the test's thread.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::time::Duration;

use afd_dragonfly::Subscription;
use backon::ExponentialBuilder;
use tokio::time::Instant;

use crate::fake_redis::{FakeRedis, Reply, push_frame};
use crate::hub_gap_faults::{
    BUDGET, FIRST, RETRY, deliver, fake_and_hub, fake_and_hub_on, gap_on, subscribes_seen,
};
use crate::recorder::Recorder;

/// A slot map at the fake's own address that owns every slot but `channel`'s,
/// so the driver can route through it and the hub finds the channel unowned.
fn map_without(fake: &FakeRedis, channel: &str) -> &'static str {
    let port = fake
        .url()
        .rsplit(':')
        .next()
        .expect("the url names a port")
        .to_owned();
    let slot = crc16::State::<crc16::XMODEM>::calculate(channel.as_bytes()) % 16_384;
    let range =
        |first: u16, last: u16| format!("*3\r\n:{first}\r\n:{last}\r\n*2\r\n$0\r\n\r\n:{port}\r\n");
    let ranges: Vec<String> = [
        (0, slot.checked_sub(1)),
        (slot + 1, (slot < 16_383).then_some(16_383)),
    ]
    .into_iter()
    .filter_map(|(first, last)| {
        last.filter(|last| *last >= first)
            .map(|last| range(first, last))
    })
    .collect();
    Box::leak(format!("*{}\r\n{}", ranges.len(), ranges.concat()).into_boxed_str())
}

/// Waits for `check`, naming `what` if the budget runs out.
async fn until(what: &str, mut check: impl FnMut() -> bool) {
    let deadline = Instant::now() + BUDGET;
    while !check() {
        assert!(Instant::now() < deadline, "{what} did not happen");
        tokio::time::sleep(RETRY).await;
    }
}

/// How many times the fake was sent `command`.
fn sent(fake: &FakeRedis, command: &str) -> usize {
    fake.seen().iter().filter(|name| *name == command).count()
}

/// Nothing is waiting on `reader` right now.
async fn quiet(reader: &mut Subscription) {
    let waiting = tokio::time::timeout(Duration::from_millis(100), reader.recv()).await;
    assert!(waiting.is_err(), "nothing was delivered: {waiting:?}");
}

/// A push of a kind the hub never subscribes with — a plain `message`, as a
/// non-sharded `SUBSCRIBE` would earn — reaches no reader and costs no gap,
/// no re-subscribe and no redial.
#[tokio::test(flavor = "multi_thread")]
async fn a_push_of_a_kind_the_hub_never_asked_for_is_ignored() {
    let (fake, hub) = fake_and_hub().await;
    let mut first = hub.subscribe(FIRST);
    deliver(&fake, FIRST, "primed", &mut first).await;
    let subscribed = subscribes_seen(&fake);

    fake.push(push_frame(&["message", FIRST, "not-sharded"]));
    quiet(&mut first).await;

    assert_eq!(
        deliver(&fake, FIRST, "after", &mut first).await,
        0,
        "no gap"
    );
    assert_eq!(subscribes_seen(&fake), subscribed, "nothing re-subscribed");
    assert_eq!(hub.connections_opened(), 1, "nothing redialled");
}

/// A repair that finds no range owning a channel does not guess where to
/// send its subscribe: the connection is redialled whole, logged as a failed
/// command, and the channel's reader is told about its gap.
#[tokio::test]
async fn a_channel_no_range_owns_is_redialled_rather_than_guessed() {
    let recorder = Recorder::install();
    let (fake, hub) = fake_and_hub().await;
    let mut first = hub.subscribe(FIRST);
    deliver(&fake, FIRST, "primed", &mut first).await;

    fake.set_reply("CLUSTER SLOTS", Reply::Raw(map_without(&fake, FIRST)));
    fake.cut();
    until("the repair to give up on the unowned channel", || {
        recorder.events().iter().any(|event| {
            event
                .fields
                .get("event")
                .is_some_and(|name| name == "hub_connection_dropped")
        })
    })
    .await;
    fake.set_reply("CLUSTER SLOTS", Reply::ClusterSlots);

    until("the hub to redial", || hub.connections_opened() >= 2).await;
    gap_on(&mut first).await;
    deliver(&fake, FIRST, "resumed", &mut first).await;
    let dropped = recorder
        .events()
        .into_iter()
        .find(|event| {
            event
                .fields
                .get("event")
                .is_some_and(|name| name == "hub_connection_dropped")
        })
        .expect("the redial is logged");
    assert_eq!(
        dropped.fields.get("cause").map(String::as_str),
        Some("command_failed")
    );
}

/// An owner that never answers as itself is not subscribed through anyone
/// else: the hub keeps asking until the repair window closes, then redials
/// whole, and the reader is told about its gap.
#[tokio::test(flavor = "multi_thread")]
async fn an_owner_that_never_answers_is_redialled_when_the_window_closes() {
    let (fake, hub) = fake_and_hub().await;
    let mut first = hub.subscribe(FIRST);
    deliver(&fake, FIRST, "primed", &mut first).await;
    let named = format!(
        "*1\r\n*3\r\n:0\r\n:16383\r\n*3\r\n$0\r\n\r\n:{}\r\n$5\r\nowner\r\n",
        fake.url().rsplit(':').next().expect("the url names a port")
    );
    fake.set_reply(
        "CLUSTER SLOTS",
        Reply::Raw(Box::leak(named.into_boxed_str())),
    );
    fake.set_reply("CLUSTER MYID", Reply::Raw("+someone-else\r\n"));
    let asked = sent(&fake, "CLUSTER");

    fake.cut();
    until("the repair to ask who answers, twice", || {
        sent(&fake, "CLUSTER") > asked + 2
    })
    .await;
    // The driver's own replay may re-send the subscribe on a one-node fake;
    // what must not happen inside the window is the hub giving up early.
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_eq!(hub.connections_opened(), 1, "still inside the window");

    let window = Instant::now() + Duration::from_secs(8);
    while hub.connections_opened() < 2 {
        assert!(
            Instant::now() < window,
            "the window closed without a redial"
        );
        tokio::time::sleep(RETRY).await;
    }
    gap_on(&mut first).await;
    deliver(&fake, FIRST, "resumed", &mut first).await;
}

/// A caller's schedule that gives up after one retry does not end the redial:
/// the same schedule starts again until the cluster answers, and the reader
/// resumes on the same subscription.
#[tokio::test(flavor = "multi_thread")]
async fn a_redial_outlives_a_schedule_with_an_attempt_limit() {
    let capped = ExponentialBuilder::new()
        .with_min_delay(Duration::from_millis(5))
        .with_max_delay(Duration::from_millis(10))
        .with_max_times(1);
    let (fake, hub) = fake_and_hub_on(capped).await;
    let mut first = hub.subscribe(FIRST);
    deliver(&fake, FIRST, "primed", &mut first).await;

    // A handshake the server hangs up on fails every dial; an unsubscribe it
    // hangs up on is the failed command that starts the redial.
    fake.set_reply("CLUSTER", Reply::Hangup);
    fake.set_reply("SUNSUBSCRIBE", Reply::Hangup);
    let asked = sent(&fake, "CLUSTER");
    drop(hub.subscribe(crate::hub_gap_faults::SECOND));
    until("more dials than the schedule allows", || {
        sent(&fake, "CLUSTER") >= asked + 4
    })
    .await;
    assert_eq!(hub.connections_opened(), 1, "every one of them failed");

    fake.set_reply("CLUSTER", Reply::ClusterSlots);
    until("the hub to redial", || hub.connections_opened() >= 2).await;
    gap_on(&mut first).await;
    deliver(&fake, FIRST, "resumed", &mut first).await;
}

/// The push a node sends when a slot moves away — `sunsubscribe` for a
/// channel a reader holds — is re-subscribed at once on the same connection,
/// and the reader is told about the gap. The same push for a channel nobody
/// holds is the echo of the hub's own unsubscribe, and is left alone.
#[tokio::test]
async fn a_slot_moved_under_a_held_channel_is_re_subscribed_and_gapped() {
    let recorder = Recorder::install();
    let (fake, hub) = fake_and_hub().await;
    let mut first = hub.subscribe(FIRST);
    deliver(&fake, FIRST, "primed", &mut first).await;
    let subscribed = subscribes_seen(&fake);

    fake.push(push_frame(&["sunsubscribe", "fleet:nobody:activity", "0"]));
    fake.push(push_frame(&["sunsubscribe", FIRST, "0"]));
    gap_on(&mut first).await;
    deliver(&fake, FIRST, "resumed", &mut first).await;

    assert_eq!(
        subscribes_seen(&fake),
        subscribed + 1,
        "only the held channel"
    );
    assert_eq!(hub.connections_opened(), 1, "a slot move is not a redial");
    let moved = recorder.events().into_iter().any(|event| {
        event
            .fields
            .get("event")
            .is_some_and(|name| name == "hub_subscription_moved")
    });
    assert!(moved, "the move is logged");
}

/// A node lost while a gap warning is still gathering does not swallow the
/// warning: the slot move is written under its own cause, the node's repair
/// under its own, and nothing is redialled.
#[tokio::test]
async fn a_node_lost_while_a_gap_warning_gathers_keeps_both_causes() {
    let recorder = Recorder::install();
    let (fake, hub) = fake_and_hub().await;
    let mut first = hub.subscribe(FIRST);
    deliver(&fake, FIRST, "primed", &mut first).await;

    fake.push(push_frame(&["sunsubscribe", FIRST, "0"]));
    gap_on(&mut first).await;
    fake.cut();
    gap_on(&mut first).await;
    deliver(&fake, FIRST, "resumed", &mut first).await;

    let causes = || -> Vec<String> {
        recorder
            .events()
            .into_iter()
            .filter(|event| {
                event
                    .fields
                    .get("event")
                    .is_some_and(|name| name == "hub_channel_gap")
            })
            .filter_map(|event| event.fields.get("cause").cloned())
            .collect()
    };
    until("both warnings to be written", || causes().len() >= 2).await;
    let written = causes();
    assert!(written.contains(&"slot_moved".to_owned()), "{written:?}");
    assert!(written.contains(&"node_repaired".to_owned()), "{written:?}");
    assert_eq!(hub.connections_opened(), 1, "a repair is not a redial");
}

/// A lost socket that nothing confirms back — here, a hub holding no channel
/// has nothing to re-subscribe — is redialled when the repair window closes,
/// and logged as unexplained.
#[tokio::test]
async fn a_loss_nothing_confirms_is_redialled_when_its_window_closes() {
    let recorder = Recorder::install();
    let (fake, hub) = fake_and_hub().await;
    fake.cut();
    let window = Instant::now() + Duration::from_secs(8);
    while hub.connections_opened() < 2 {
        assert!(
            Instant::now() < window,
            "the window closed without a redial"
        );
        tokio::time::sleep(RETRY).await;
    }
    let cause = recorder.events().into_iter().find_map(|event| {
        (event.fields.get("event")? == "hub_connection_dropped")
            .then(|| event.fields.get("cause").cloned())
            .flatten()
    });
    assert_eq!(cause.as_deref(), Some("unexplained"));
}
