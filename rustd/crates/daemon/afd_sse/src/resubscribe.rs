//! A subscription that comes back after a loss, and the fleet tail built on
//! it: the tail announces every gap, and a subscribe that fails is announced
//! and retried.
//!
//! ```text
//! hello ─► catching_up(0) ─► subscribed ─► frames … ─► bus closed: stream ends
//!                       ├──► failed ─► backoff ─► subscribe ─► catching_up(0)
//!                       └──► bus closed: stream ends
//! ```
//!
//! The opening `catching_up` covers frames published before the interest
//! reached the publisher's node: the UI backfills from Postgres on it. A
//! reconnect arrives as a `Gap`, so a subscription errors only when the bus
//! itself closes, at drain; the tail then ends and the client reconnects to a
//! live replica rather than holding a dead one open.
//!
//! [`Link`] is the one retry state machine. The fleet tail waits out each
//! backoff; the draft stream checks between its durable reads instead.

use std::time::Duration;

use afd_bus::{Received, Subject, Subscription};
use backon::{BackoffBuilder as _, ExponentialBackoff, ExponentialBuilder};
use futures_util::stream::{self, Stream};
use tokio::time::Instant;

use crate::frame::Frame;
use crate::live::{Live, Subscribed};

/// How a lost subscription retries: doubling from the first wait to the
/// ceiling, for as long as the stream lives.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Resubscribe(ExponentialBuilder);

impl Default for Resubscribe {
    fn default() -> Self {
        Self::between(Duration::from_millis(250), Duration::from_secs(5))
    }
}

impl Resubscribe {
    /// Waits starting at `first` and doubling to `ceiling`.
    pub(crate) const fn between(first: Duration, ceiling: Duration) -> Self {
        Self(
            ExponentialBuilder::new()
                .with_factor(2.0)
                .with_min_delay(first)
                .with_max_delay(ceiling)
                .without_max_times(),
        )
    }

    /// The waits between failed attempts, from the first.
    pub(crate) fn schedule(self) -> ExponentialBackoff {
        self.0.build()
    }
}

/// Where a link stands.
#[derive(Debug)]
enum State {
    Open(Subscription),
    /// Lost or never opened; subscribe attempt `attempt` is due at `at`.
    Retrying {
        attempt: u32,
        at: Instant,
    },
    /// The bus shut down and never answers again.
    Closed,
}

/// One subject's subscription, reopened on a backoff after it is lost. A
/// closed bus is final: the link never retries it.
#[derive(Debug)]
pub struct Link {
    live: Live,
    subject: Subject,
    state: State,
    backoff: ExponentialBackoff,
}

impl Link {
    /// Subscribe to `subject` once; a failure leaves the link retrying.
    pub(crate) async fn open(live: Live, subject: Subject) -> Self {
        let first = live.try_subscribe(&subject, 0).await;
        let mut link = Self {
            backoff: live.resubscribe().schedule(),
            live,
            subject,
            state: State::Closed,
        };
        link.settle(first, 0);
        link
    }

    fn settle(&mut self, subscribed: Subscribed, attempt: u32) {
        self.state = match subscribed {
            Subscribed::Open(subscription) => {
                self.backoff = self.live.resubscribe().schedule();
                State::Open(subscription)
            }
            Subscribed::Unavailable => State::Retrying {
                attempt: attempt.saturating_add(1),
                // The schedule has no last attempt, so it always has a wait.
                at: Instant::now() + self.backoff.next().unwrap_or_default(),
            },
            Subscribed::Closed => State::Closed,
        };
    }

    /// The live subscription, or `None` while retrying or once closed.
    pub fn subscription(&mut self) -> Option<&mut Subscription> {
        match &mut self.state {
            State::Open(subscription) => Some(subscription),
            State::Retrying { .. } | State::Closed => None,
        }
    }

    /// Whether the bus shut down; a closed link stays closed.
    #[must_use]
    pub const fn is_closed(&self) -> bool {
        matches!(self.state, State::Closed)
    }

    /// Record that the open subscription failed with `error`. A closed bus
    /// is final; any other loss is retried at once.
    pub fn lose(&mut self, error: &afd_bus::Error) {
        self.state = if error.is_closed() {
            State::Closed
        } else {
            State::Retrying {
                attempt: 0,
                at: Instant::now(),
            }
        };
    }

    /// Subscribe again once the pending wait has passed. `true` when this
    /// call reopened the subscription.
    pub async fn reopen_if_due(&mut self) -> bool {
        let State::Retrying { attempt, at } = self.state else {
            return false;
        };
        if Instant::now() < at {
            return false;
        }
        let subscribed = self.live.try_subscribe(&self.subject, attempt).await;
        self.settle(subscribed, attempt);
        if matches!(self.state, State::Open(_)) {
            tracing::debug!(attempt, event = "sse_resubscribed");
            return true;
        }
        false
    }

    /// Wait until the pending retry is due; at once when none is pending.
    async fn until_due(&mut self) {
        if let State::Retrying { attempt, at } = self.state {
            let wait_ms = at.saturating_duration_since(Instant::now()).as_millis();
            tracing::debug!(attempt, wait_ms, event = "sse_resubscribing");
            tokio::time::sleep_until(at).await;
        }
    }
}

/// What one turn of the tail decided.
enum Turn {
    /// Send this frame; the next activity frame is numbered `seq`.
    Emit(Frame, u64),
    /// Nothing to send yet.
    Again,
    /// The bus closed: the stream ends.
    End,
}

/// The tail after `hello` and its first `catching_up`: every later gap
/// announced, a lost subscription retried until the bus closes.
pub(crate) fn resilient(link: Link) -> impl Stream<Item = Frame> + Send {
    stream::unfold((link, 0_u64), async |(mut link, seq)| {
        loop {
            match turn(&mut link, seq).await {
                Turn::Emit(frame, next) => return Some((frame, (link, next))),
                Turn::Again => {}
                Turn::End => return None,
            }
        }
    })
}

async fn turn(link: &mut Link, seq: u64) -> Turn {
    if link.is_closed() {
        // A closed bus never answers again: end the stream, so the client
        // reconnects to a replica that is serving.
        tracing::debug!(event = "sse_tail_closed", reason = "closed");
        return Turn::End;
    }
    let Some(subscription) = link.subscription() else {
        link.until_due().await;
        // Frames published while the tail was down are gone: the viewer
        // backfills from the events list, and numbering starts over.
        return if link.reopen_if_due().await {
            Turn::Emit(Frame::catching_up(0), 0)
        } else {
            Turn::Again
        };
    };
    match subscription.recv().await {
        Ok(Received::Message(message)) => {
            Turn::Emit(Frame::activity(seq, message), seq.wrapping_add(1))
        }
        Ok(Received::Lagged(missed)) => Turn::Emit(Frame::catching_up(missed), seq),
        Ok(Received::Gap) => Turn::Emit(Frame::catching_up(0), seq),
        Err(lost) => {
            let error_code = lost.code().as_str();
            let reason = lost.reason();
            tracing::debug!(event = "sse_tail_lost", error_code, reason);
            link.lose(&lost);
            Turn::Again
        }
    }
}

#[cfg(test)]
#[path = "resubscribe_tests.rs"]
mod tests;
