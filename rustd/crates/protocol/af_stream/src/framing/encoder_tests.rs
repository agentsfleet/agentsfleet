//! An encoder charges its frame's exact length once, and holds no part of a
//! charge while it waits for room.

use std::time::Duration;

use af_wire::protocol::{MESSAGE_BYTES_MAX, StreamKind};
use tokio::time::{Instant, timeout};
use tokio_util::sync::CancellationToken;

use super::Encoder;
use crate::{Budget, OwnedFrame, Result, error::ErrorKind};

const MIB: usize = 1024 * 1024;

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(5)
}

/// A string serializing to exactly `bytes`, quotes included.
fn value_of(bytes: usize) -> String {
    "x".repeat(bytes - 2)
}

async fn finished(encoder: Encoder) -> Result<OwnedFrame> {
    encoder.finish(&CancellationToken::new(), deadline()).await
}

#[tokio::test]
async fn a_frame_the_pool_has_room_for_is_charged_its_exact_length_at_once() -> Result<()> {
    let budget = Budget::default();
    let available = budget.available(StreamKind::Work);
    let mut encoder = Encoder::new(&budget, StreamKind::Work);
    assert_eq!(budget.available(StreamKind::Work), available);
    encoder.frame(&"assignment")?;
    assert_eq!(available - budget.available(StreamKind::Work), 12);
    let frame = finished(encoder).await?;
    assert_eq!(frame.as_bytes(), br#""assignment""#);
    assert_eq!(available - budget.available(StreamKind::Work), 12);
    drop(frame);
    assert_eq!(budget.available(StreamKind::Work), available);
    Ok(())
}

/// With too little room, the encoder waits for its whole length and holds
/// none of the pool meanwhile.
#[tokio::test]
async fn a_frame_without_room_waits_for_its_whole_length_holding_nothing() -> Result<()> {
    let budget = Budget::new(MIB, 4 * MIB)?;
    let occupying = crate::encode(
        &value_of(3 * MIB),
        &budget,
        StreamKind::Work,
        &CancellationToken::new(),
        deadline(),
    )
    .await?;
    let mut encoder = Encoder::new(&budget, StreamKind::Work);
    encoder.frame(&value_of(2 * MIB))?;
    assert_eq!(budget.available(StreamKind::Work), MIB, "nothing held");
    let finishing = tokio::spawn(finished(encoder));
    tokio::task::yield_now().await;
    assert!(!finishing.is_finished());
    drop(occupying);
    let frame = timeout(Duration::from_secs(5), finishing)
        .await
        .map_err(|_elapsed| ErrorKind::Protocol)?
        .map_err(|_join| ErrorKind::Protocol)??;
    assert_eq!(frame.as_bytes().len(), 2 * MIB);
    assert_eq!(budget.available(StreamKind::Work), 2 * MIB);
    drop(frame);
    assert_eq!(budget.available(StreamKind::Work), 4 * MIB);
    Ok(())
}

/// Encoders that each need most of the pool all finish as earlier frames
/// are written, and a small frame queued among them is sent too: none holds
/// a slice of the pool while waiting for the rest.
#[tokio::test]
async fn concurrent_large_frames_neither_deadlock_nor_starve_a_small_one() -> Result<()> {
    let budget = Budget::new(MIB, 4 * MIB)?;
    let mut sends = tokio::task::JoinSet::new();
    for _ in 0..8 {
        let mut encoder = Encoder::new(&budget, StreamKind::Work);
        encoder.frame(&value_of(3 * MIB))?;
        sends.spawn(async move {
            let frame = finished(encoder).await?;
            // The writer flushes the frame, then its charge returns.
            tokio::task::yield_now().await;
            Ok::<_, crate::Error>(frame.as_bytes().len())
        });
    }
    let small = budget.clone();
    sends.spawn(async move {
        let frame = crate::encode(
            &"heartbeat",
            &small,
            StreamKind::Work,
            &CancellationToken::new(),
            deadline(),
        )
        .await?;
        Ok(frame.as_bytes().len())
    });
    let lengths = timeout(Duration::from_secs(10), sends.join_all())
        .await
        .map_err(|_elapsed| ErrorKind::Protocol)?
        .into_iter()
        .collect::<Result<Vec<_>>>()?;
    assert_eq!(lengths.len(), 9, "every frame was charged and sent");
    assert_eq!(lengths.iter().filter(|&&bytes| bytes == 3 * MIB).count(), 8);
    assert!(
        lengths.contains(&11),
        "the small frame was sent: {lengths:?}"
    );
    assert_eq!(budget.available(StreamKind::Work), 4 * MIB);
    Ok(())
}

#[tokio::test]
async fn a_wait_for_room_is_cancellable_and_leaves_the_pool_whole() -> Result<()> {
    let budget = Budget::new(MIB, 2 * MIB)?;
    let stop = CancellationToken::new();
    let mut encoder = Encoder::new(&budget, StreamKind::Work);
    encoder.frame(&value_of(3 * MIB))?;
    let finishing = encoder.finish(&stop, deadline());
    tokio::pin!(finishing);
    assert!(
        std::future::poll_fn(|cx| std::task::Poll::Ready(finishing.as_mut().poll(cx).is_pending()))
            .await
    );
    stop.cancel();
    let refused = finishing.await.err().ok_or(ErrorKind::Protocol)?;
    assert_eq!(refused.reason(), crate::Reason::Cancelled);
    assert_eq!(budget.available(StreamKind::Work), 2 * MIB);
    Ok(())
}

#[tokio::test]
async fn a_value_past_the_frame_ceiling_is_refused_and_charges_nothing() -> Result<()> {
    let budget = Budget::default();
    let available = budget.available(StreamKind::Work);
    let mut encoder = Encoder::new(&budget, StreamKind::Work);
    let refused = encoder
        .frame(&"x".repeat(MESSAGE_BYTES_MAX))
        .err()
        .ok_or(ErrorKind::Protocol)?;
    assert_eq!(refused.reason(), crate::Reason::Protocol);
    drop(encoder);
    assert_eq!(budget.available(StreamKind::Work), available);
    Ok(())
}

#[tokio::test]
async fn finishing_without_a_frame_is_refused() -> Result<()> {
    let budget = Budget::default();
    let available = budget.available(StreamKind::Work);
    let refused = finished(Encoder::new(&budget, StreamKind::Work))
        .await
        .err()
        .ok_or(ErrorKind::Protocol)?;
    assert_eq!(refused.reason(), crate::Reason::Protocol);
    assert!(
        std::error::Error::source(&refused)
            .is_some_and(<dyn std::error::Error>::is::<garde::Report>)
    );
    assert_eq!(budget.available(StreamKind::Work), available);
    Ok(())
}
