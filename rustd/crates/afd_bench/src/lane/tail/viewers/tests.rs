//! A viewer whose tail ends early keeps what it saw.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking on an unmet precondition"
)]

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use afd_dragonfly::Message;
use afd_sse::Frame;
use futures_util::StreamExt as _;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

use super::{FRAMES, Shared, view};
use crate::lane::tail::publish::{PROBE, frame_of};

/// A frame as the tail would hand it over, carrying `payload`.
fn frame(payload: String) -> Frame {
    Frame::activity(
        1,
        Arc::new(Message {
            channel: "bench".to_owned(),
            payload,
        }),
    )
}

fn shared() -> Arc<Shared> {
    Arc::new(Shared {
        epoch: Instant::now(),
        published_at: (0..FRAMES).map(|_| AtomicU64::new(0)).collect(),
        delivered: AtomicU64::new(0),
        ready: AtomicU64::new(0),
        progress: Notify::new(),
    })
}

#[tokio::test]
async fn a_tail_that_ends_early_reports_the_frames_and_lags_it_saw() {
    let shared = shared();
    // Two probes, two measured frames, a lag notice, a frame of no interest,
    // then the tail closes — well short of the rung's hundred.
    let tail = futures_util::stream::iter([
        frame(PROBE.to_owned()),
        frame(PROBE.to_owned()),
        frame(frame_of(200)),
        Frame::catching_up(3),
        frame(frame_of(200)),
        frame(r#"{"kind":"unrelated"}"#.to_owned()),
    ])
    .boxed();

    let seen = view(tail, Arc::clone(&shared), CancellationToken::new())
        .await
        .expect("an ended tail is a short count, not a failure");

    assert_eq!(seen.frames, 2, "only measured frames count as delivered");
    assert_eq!(seen.lagged, 1, "a lag notice is counted, not delivered");
    assert_eq!(seen.latency.count(), 2);
    assert_eq!(shared.delivered.load(Ordering::Relaxed), 2);
    assert_eq!(
        shared.ready.load(Ordering::Relaxed),
        1,
        "a viewer is ready once, however many probes it hears"
    );
}
