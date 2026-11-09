//! A fleet tail that never goes silent: it announces every gap, and a
//! subscribe that fails is announced and retried (M244 §6).
//!
//! ```text
//! hello ─► catching_up(0) ─► subscribed ─► frames … ─► bus closed: stream ends
//!                       └──► failed ─► backoff ─► subscribe ─► subscribed
//! ```
//!
//! The opening `catching_up` covers frames published before the interest
//! reached the publisher's node: the UI backfills from Postgres on it. A
//! reconnect arrives as a `Gap`, so a subscription only errors when the bus
//! itself closes, at drain; the stream then ends and the client reconnects to
//! a live replica rather than holding a dead one open.

use std::time::Duration;

use afd_bus::{Received, Subject, Subscription};
use futures_util::stream::{self, Stream};

use crate::frame::Frame;
use crate::live::{Live, Subscribed};

/// How a lost tail retries its subscription.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Resubscribe {
    /// The first wait after a failed subscribe.
    pub first: Duration,
    /// The longest wait between attempts.
    pub ceiling: Duration,
}

impl Default for Resubscribe {
    fn default() -> Self {
        Self {
            first: Duration::from_millis(250),
            ceiling: Duration::from_secs(5),
        }
    }
}

impl Resubscribe {
    /// The wait before attempt `attempt` (0-based), doubling to the ceiling.
    #[must_use]
    pub fn wait(&self, attempt: u32) -> Duration {
        self.first
            .saturating_mul(2_u32.saturating_pow(attempt.min(16)))
            .min(self.ceiling)
    }
}

enum State {
    /// Waiting to subscribe; the opening `catching_up` already told the viewer.
    Subscribing { attempt: u32 },
    Tailing {
        subscription: Subscription,
        seq: u64,
    },
}

/// The tail after `hello` and its first `catching_up`: every later gap
/// announced, a failed subscribe retried. `first` is the subscription the caller
/// confirmed before announcing the stream, or `None` when that failed.
pub(crate) fn resilient(
    live: Live,
    subject: Subject,
    first: Option<Subscription>,
) -> impl Stream<Item = Frame> + Send {
    let start = match first {
        Some(subscription) => State::Tailing {
            subscription,
            seq: 0,
        },
        // The opening `catching_up` already told the viewer about this loss.
        None => State::Subscribing { attempt: 1 },
    };
    stream::unfold((live, subject, start), async |(live, subject, state)| {
        let mut state = state;
        loop {
            state = match state {
                State::Subscribing { attempt } => {
                    let wait = live.resubscribe().wait(attempt.saturating_sub(1));
                    let wait_ms = wait.as_millis();
                    tracing::debug!(attempt, wait_ms, event = "sse_tail_resubscribing");
                    tokio::time::sleep(wait).await;
                    match live.try_subscribe(&subject).await {
                        // Frames published while the tail was down are gone:
                        // the viewer backfills from the events list.
                        Subscribed::Open(subscription) => {
                            tracing::debug!(attempt, event = "sse_tail_resubscribed");
                            let next = State::Tailing {
                                subscription,
                                seq: 0,
                            };
                            return Some((Frame::catching_up(0), (live, subject, next)));
                        }
                        Subscribed::Unavailable => State::Subscribing {
                            attempt: attempt.saturating_add(1),
                        },
                        // A closed bus never answers again: end the stream, so
                        // the client reconnects to a replica that is serving.
                        Subscribed::Closed => {
                            tracing::debug!(event = "sse_tail_closed", reason = "closed");
                            return None;
                        }
                    }
                }
                State::Tailing {
                    mut subscription,
                    seq,
                } => match subscription.recv().await {
                    Ok(Received::Message(message)) => {
                        let next = State::Tailing {
                            subscription,
                            seq: seq.wrapping_add(1),
                        };
                        return Some((Frame::activity(seq, message), (live, subject, next)));
                    }
                    Ok(Received::Lagged(missed)) => {
                        let next = State::Tailing { subscription, seq };
                        return Some((Frame::catching_up(missed), (live, subject, next)));
                    }
                    Ok(Received::Gap) => {
                        let next = State::Tailing { subscription, seq };
                        return Some((Frame::catching_up(0), (live, subject, next)));
                    }
                    Err(closed) => {
                        let error_code = closed.code().as_str();
                        let reason = closed.reason();
                        tracing::debug!(event = "sse_tail_closed", error_code, reason);
                        return None;
                    }
                },
            };
        }
    })
}

#[cfg(test)]
#[path = "resubscribe_tests.rs"]
mod tests;
