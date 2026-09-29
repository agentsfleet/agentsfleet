//! What one fleet's tail turns each thing a subscription reports into.
#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use afd_dragonfly::SubscriptionHub;
use futures_util::StreamExt as _;

use super::tail;
use crate::frame::{Frame, KIND_CATCHING_UP};

const CHANNEL: &str = "fleet:tail:activity";
const CHUNK: &str = r#"{"kind":"chunk","text":"hi"}"#;

/// Frames past a reader's buffer, which is 256 deep: these many are lost.
const OVERFLOW: u64 = 44;

async fn next(stream: &mut (impl futures_util::Stream<Item = Frame> + Unpin)) -> Frame {
    stream.next().await.expect("the tail stays open")
}

/// A subscription lost and restored reaches the client as `catching_up` with
/// nothing counted — nobody saw what was missed — and spends no sequence
/// number, so the ids stay gapless over what the client received.
#[tokio::test]
async fn a_gap_is_catching_up_with_nothing_counted_and_no_sequence_spent() {
    let (hub, server) = SubscriptionHub::detached();
    let mut frames = Box::pin(tail(hub.subscribe(CHANNEL)));
    server.confirm(CHANNEL);
    server.publish(CHANNEL, CHUNK);
    assert_eq!(next(&mut frames).await.seq, 0);

    server.confirm(CHANNEL);
    let gap = next(&mut frames).await;
    assert_eq!((gap.seq, gap.kind.as_ref()), (0, KIND_CATCHING_UP));
    assert_eq!(gap.data, r#"{"kind":"catching_up","dropped":0}"#);

    server.publish(CHANNEL, CHUNK);
    assert_eq!(next(&mut frames).await.seq, 1, "the gap spent no id");
}

/// A reader that fell behind is told how many frames it lost, then resumes
/// at the next id.
#[tokio::test]
async fn a_lag_is_catching_up_with_the_count_it_lost() {
    let (hub, server) = SubscriptionHub::detached();
    let mut frames = Box::pin(tail(hub.subscribe(CHANNEL)));
    for _ in 0..(256 + OVERFLOW) {
        server.publish(CHANNEL, CHUNK);
    }
    let lag = next(&mut frames).await;
    assert_eq!(lag.kind, KIND_CATCHING_UP);
    assert_eq!(
        lag.data,
        format!(r#"{{"kind":"catching_up","dropped":{OVERFLOW}}}"#)
    );
    assert_eq!(next(&mut frames).await.seq, 0, "the lag spent no id");
}

/// A hub shut down ends the tail rather than leaving it waiting.
#[tokio::test]
async fn a_closed_hub_ends_the_tail() {
    let (hub, _server) = SubscriptionHub::detached();
    let mut frames = Box::pin(tail(hub.subscribe(CHANNEL)));
    hub.shutdown();
    assert!(frames.next().await.is_none());
}
