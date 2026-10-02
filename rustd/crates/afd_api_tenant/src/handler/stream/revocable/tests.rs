//! The ending both live streams share.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use afd_sse::{Frame, KIND_ACCESS_REVOKED, KIND_CATCHING_UP};
use futures_util::StreamExt as _;

use super::{Turn, until_revoked};

/// The kinds a stream sent, in order.
async fn kinds(stream: futures_util::stream::BoxStream<'static, Frame>) -> Vec<String> {
    stream.map(|frame| frame.kind.into_owned()).collect().await
}

/// Frames go out until a turn reports the access gone; `access_revoked` is
/// then the last frame, and no turn runs after it.
#[tokio::test]
async fn should_send_access_revoked_last_and_run_no_turn_after_it() {
    let turns = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&turns);
    let stream = until_revoked(0_u8, move |sent| {
        counted.fetch_add(1, Ordering::SeqCst);
        async move {
            Some(if sent < 2 {
                Turn::Frame(Frame::catching_up(0), sent + 1)
            } else {
                Turn::Revoked
            })
        }
    });
    assert_eq!(
        kinds(stream).await,
        [KIND_CATCHING_UP, KIND_CATCHING_UP, KIND_ACCESS_REVOKED]
    );
    assert_eq!(
        turns.load(Ordering::SeqCst),
        3,
        "nothing runs after the end"
    );
}

/// A stream whose frames run out ends without claiming the access was lost.
#[tokio::test]
async fn should_end_quietly_when_the_frames_run_out() {
    let stream = until_revoked((), |()| async { None });
    assert!(kinds(stream).await.is_empty());
}
