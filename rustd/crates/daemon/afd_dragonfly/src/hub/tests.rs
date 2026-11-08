//! The channel table without a socket: fan-out sharing and the gap rule.
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use std::sync::Arc;

use futures_util::FutureExt as _;
use tokio::sync::mpsc;

use super::channels::{Command, Confirmation, HubInner};
use super::{Message, Received, Subscription, SubscriptionHub};

const CHANNEL: &str = "fleet:a:activity";
const OTHER: &str = "fleet:b:activity";

/// A hub with no connection: commands queue on a receiver the test holds.
fn detached() -> (SubscriptionHub, mpsc::UnboundedReceiver<Command>) {
    let (commands, receiver) = mpsc::unbounded_channel();
    let hub = SubscriptionHub {
        inner: Arc::new(HubInner::new()),
        commands,
    };
    (hub, receiver)
}

fn message(payload: &str) -> Message {
    Message {
        channel: CHANNEL.to_owned(),
        payload: payload.to_owned(),
    }
}

/// What a reader has waiting right now, without waiting for more.
fn ready(reader: &mut Subscription) -> Option<Received> {
    reader
        .recv()
        .now_or_never()
        .map(|received| received.expect("the hub is open"))
}

/// Dimension 5.4: one message reaches three readers as ONE allocation.
#[tokio::test]
async fn test_fanout_shares_one_payload() {
    let (hub, _commands) = detached();
    let mut readers: Vec<_> = (0..3).map(|_| hub.subscribe(CHANNEL)).collect();
    hub.inner.dispatch(message("{\"kind\":\"chunk\"}"));

    let delivered: Vec<Arc<Message>> = readers
        .iter_mut()
        .map(|reader| match ready(reader) {
            Some(Received::Message(message)) => message,
            other => panic!("each reader receives the frame, got {other:?}"),
        })
        .collect();
    let [first, second, third] = delivered.as_slice() else {
        panic!("three readers, three deliveries");
    };
    assert!(Arc::ptr_eq(first, second) && Arc::ptr_eq(second, third));
    assert_eq!(first.payload.as_ptr(), third.payload.as_ptr());
    assert_eq!(
        Arc::strong_count(first),
        3,
        "the readers hold the only references: nothing kept a copy"
    );
}

/// The first confirmation opens the subscription quietly; a repeat gaps
/// every reader of that channel and no other.
#[tokio::test]
async fn a_repeated_confirmation_gaps_only_its_own_channel() {
    let (hub, _commands) = detached();
    let mut first = hub.subscribe(CHANNEL);
    let mut second = hub.subscribe(CHANNEL);
    let mut elsewhere = hub.subscribe(OTHER);

    assert_eq!(hub.inner.confirm(CHANNEL), Confirmation::First);
    assert_eq!(hub.inner.confirm(OTHER), Confirmation::First);
    assert!(
        ready(&mut first).is_none(),
        "a first confirmation is no gap"
    );

    assert_eq!(hub.inner.confirm(CHANNEL), Confirmation::Replay);
    assert_eq!(ready(&mut first), Some(Received::Gap));
    assert_eq!(ready(&mut second), Some(Received::Gap));
    assert!(
        ready(&mut elsewhere).is_none(),
        "another channel lost nothing"
    );
}

/// A confirmation for a channel nobody holds tells nobody.
#[tokio::test]
async fn a_confirmation_for_a_released_channel_is_unheld() {
    let (hub, mut commands) = detached();
    drop(hub.subscribe(CHANNEL));
    assert_eq!(hub.inner.confirm(CHANNEL), Confirmation::Unheld);

    let queued: Vec<_> = std::iter::from_fn(|| commands.try_recv().ok()).collect();
    assert!(
        matches!(
            queued.as_slice(),
            [Command::Subscribe(opened), Command::Unsubscribe(closed)]
                if opened == CHANNEL && closed == CHANNEL
        ),
        "subscribe then unsubscribe, in that order: {queued:?}"
    );
}

/// A channel re-opened after its last reader left starts unconfirmed, so its
/// new subscription's confirmation is not mistaken for a replay.
#[tokio::test]
async fn a_reopened_channel_starts_unconfirmed() {
    let (hub, _commands) = detached();
    let reader = hub.subscribe(CHANNEL);
    assert_eq!(hub.inner.confirm(CHANNEL), Confirmation::First);
    drop(reader);

    let mut reopened = hub.subscribe(CHANNEL);
    assert_eq!(hub.inner.confirm(CHANNEL), Confirmation::First);
    assert!(ready(&mut reopened).is_none());
}
