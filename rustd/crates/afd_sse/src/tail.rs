//! One fleet's activity, as a stream of frames.
//!
//! The per-fleet surface is the simple one: a single channel, a counter that
//! starts at zero, and frames forwarded in publish order. Everything harder —
//! several channels on one connection, a set that changes while the connection
//! is open — is the [fan-in](crate::fanin)'s problem.
//!
//! # The stream ENDS rather than raising
//!
//! By the time a frame could fail there are already headers on the wire, so
//! there is no status code left to answer with. A hub that closes ends the
//! stream, the client sees the connection close, and it reconnects — which is
//! what an `EventSource` does by itself.

use afd_redis::Subscription;
use afd_redis::hub::Received;
use futures_util::Stream;
use futures_util::stream;

use crate::frame::Frame;

/// Every frame published on `subscription`, numbered from zero.
///
/// A reader that falls behind is told so IN BAND — the dropped count arrives as
/// a `catching_up` frame rather than as silence, because a gap a client cannot
/// see is a gap it will not backfill.
pub fn tail(subscription: Subscription) -> impl Stream<Item = Frame> + Send {
    stream::unfold(
        (subscription, 0_u64),
        |(mut subscription, seq)| async move {
            match subscription.recv().await {
                Ok(Received::Message(message)) => {
                    let next = seq.wrapping_add(1);
                    Some((Frame::activity(seq, message.payload), (subscription, next)))
                }
                // A control frame, so it does not spend a sequence number: the ids
                // stay gapless over the frames the client actually received.
                Ok(Received::Lagged(missed)) => {
                    Some((Frame::catching_up(missed), (subscription, seq)))
                }
                Err(closed) => {
                    let channel = subscription.channel();
                    let reason = closed.to_string();
                    tracing::debug!(channel, reason, event = "sse_tail_closed");
                    None
                }
            }
        },
    )
}
