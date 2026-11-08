//! A held stream that closes, and a rung whose streams did not all hear.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use afd_dragonfly::Message;
use afd_sse::{Ceiling, Frame};
use futures_util::StreamExt as _;
use tokio_util::sync::CancellationToken;

use super::{UNSET, Watch, hold, latency_of};
use crate::lane::tail::publish::{PROBE, frame_of};

fn frame(payload: String) -> Frame {
    Frame::activity(
        1,
        Arc::new(Message {
            channel: "bench".to_owned(),
            payload,
        }),
    )
}

#[tokio::test]
async fn a_stream_that_closes_releases_its_slot_and_keeps_what_it_heard() {
    let ceiling = Ceiling::new(1);
    let slot = ceiling.admit();
    assert!(slot.is_some(), "an empty ceiling admits one");
    let watch = Arc::new(Watch::new(1));
    // Stamp the timed frame as published, so its arrival is recorded.
    if let Some(stamped) = watch.stamped.first() {
        stamped.store(watch.now(), Ordering::Release);
    }
    let tail = futures_util::stream::iter([
        frame(PROBE.to_owned()),
        frame(r#"{"kind":"unrelated"}"#.to_owned()),
        frame(frame_of(200)),
    ])
    .boxed();

    if let Some(slot) = slot {
        hold(tail, slot, Arc::clone(&watch), 0, CancellationToken::new()).await;
    }

    assert!(
        watch
            .heard
            .first()
            .is_some_and(|it| it.load(Ordering::Relaxed))
    );
    assert!(
        watch
            .arrived
            .first()
            .is_some_and(|it| it.load(Ordering::Acquire) != UNSET)
    );
    assert_eq!(ceiling.live(), 0, "a closed stream gives its slot back");
}

#[test]
fn a_stream_that_never_heard_its_frame_is_left_out_of_the_distribution() {
    let watch = Watch::new(3);
    if let Some(arrived) = watch.arrived.get(1) {
        arrived.store(5_000_001, Ordering::Release);
    }

    let latency = latency_of(&watch);

    assert!(
        latency.is_ok_and(|it| it.count() == 1),
        "two silent streams are counted by the rung, never timed as zero"
    );
}
