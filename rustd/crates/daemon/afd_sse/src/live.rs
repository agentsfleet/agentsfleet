//! The live surface, composed: the pub/sub connection and the instance's
//! stream budget, as one value a handler holds.
//!
//! # A deployment with no hub is a value, not a `None` every handler renders
//!
//! `afd_fleet::bundle::Bundles::unconfigured` is the shape this follows. An
//! instance with no pub/sub connection still SERVES the stream routes: the
//! connection opens, the ceiling is charged and the heartbeat runs. A fleet
//! tail tells the viewer it is catching up rather than going silent, so the
//! UI backfills from Postgres (M244 §6).

use std::collections::{BTreeMap, BTreeSet};

use afd_bus::Handle;
use futures_util::StreamExt as _;
use futures_util::stream::{self, BoxStream};

use crate::fanin::FanIn;
use crate::frame::Frame;
use crate::resubscribe::{Resubscribe, resilient};
use crate::stream_limit::{StreamLimit, StreamPermit};

/// What one subscribe attempt found.
pub(crate) enum Subscribed {
    /// The subscription is live.
    Open(afd_bus::Subscription),
    /// No bus, or one that may answer on a later attempt.
    Unavailable,
    /// The bus shut down; the stream ends and the client reconnects elsewhere.
    Closed,
}

/// What the two stream routes act through.
#[derive(Debug, Clone)]
pub struct Live {
    bus: Option<Handle>,
    stream_limit: StreamLimit,
    resubscribe: Resubscribe,
}

impl Live {
    /// The live surface of an instance holding a pub/sub connection.
    #[must_use]
    pub fn new(bus: Handle, stream_limit: StreamLimit) -> Self {
        Self {
            bus: Some(bus),
            stream_limit,
            resubscribe: Resubscribe::default(),
        }
    }

    /// Replaces how a lost tail retries, such as from a configuration file.
    #[must_use]
    pub const fn with_resubscribe(mut self, resubscribe: Resubscribe) -> Self {
        self.resubscribe = resubscribe;
        self
    }

    /// How a lost tail retries.
    #[must_use]
    pub const fn resubscribe(&self) -> &Resubscribe {
        &self.resubscribe
    }

    /// The live surface of an instance with no pub/sub connection.
    ///
    /// Its streams open, count against the ceiling, and stay silent. See the
    /// module header for why that is the behaviour rather than a refusal.
    #[must_use]
    pub fn detached(stream_limit: StreamLimit) -> Self {
        Self {
            bus: None,
            stream_limit,
            resubscribe: Resubscribe::default(),
        }
    }

    /// The hub behind this surface, for the shutdown that closes it.
    ///
    /// `None` on a detached instance, which has nothing to close.
    #[must_use]
    pub const fn bus(&self) -> Option<&Handle> {
        self.bus.as_ref()
    }

    /// A slot for one stream, or `None` when the instance is full or draining.
    #[must_use]
    pub fn admit(&self) -> Option<StreamPermit> {
        self.stream_limit.admit()
    }

    /// How many streams this instance is carrying.
    #[must_use]
    pub fn carrying(&self) -> usize {
        self.stream_limit.live()
    }

    /// How many it will carry.
    #[must_use]
    pub const fn capacity(&self) -> usize {
        self.stream_limit.capacity()
    }

    /// Share the same concurrent-stream permits with the native runner listener.
    #[must_use]
    pub const fn stream_limit(&self) -> &StreamLimit {
        &self.stream_limit
    }

    /// Subscribe before announcing a live fleet stream, then tell the viewer
    /// to backfill once: frames published before the interest reached the
    /// publisher's node are only in Postgres. A failed subscribe still opens
    /// the stream and keeps retrying, so the viewer is never left with only
    /// heartbeats.
    pub async fn tail_of(&self, fleet: &af_core::id::Uuid7) -> BoxStream<'static, Frame> {
        let subject = afd_bus::Subject::FleetTail(fleet.clone());
        let first = self.subscribe(&subject).await;
        let opening = [
            Frame::hello(&[fleet.to_string()], &BTreeMap::new()),
            Frame::catching_up(0),
        ];
        stream::iter(opening)
            .chain(resilient(self.clone(), subject, first))
            .boxed()
    }

    /// Failed subscription startup preserves the silent dashboard surface.
    pub async fn subscribe(&self, subject: &afd_bus::Subject) -> Option<afd_bus::Subscription> {
        match self.try_subscribe(subject).await {
            Subscribed::Open(subscription) => Some(subscription),
            Subscribed::Unavailable | Subscribed::Closed => None,
        }
    }

    /// Subscribe, telling a bus that may answer later from one that closed.
    pub(crate) async fn try_subscribe(&self, subject: &afd_bus::Subject) -> Subscribed {
        let Some(bus) = self.bus.as_ref() else {
            return Subscribed::Unavailable;
        };
        match bus.subscribe(subject).await {
            Ok(subscription) => Subscribed::Open(subscription),
            Err(error) => {
                let error_code = error.code().as_str();
                let reason = error.reason();
                tracing::warn!(event = "sse_subscription_unavailable", error_code, reason);
                if error.is_closed() {
                    Subscribed::Closed
                } else {
                    Subscribed::Unavailable
                }
            }
        }
    }

    /// A fan-in the caller attaches a workspace's fleets to.
    #[must_use]
    pub fn fan_in(&self) -> FanIn {
        FanIn::new(self.bus.clone())
    }

    /// The frames a fan-in carrying `fleets` would produce, as one stream.
    ///
    /// The convenience the per-fleet route gets for free: a workspace whose
    /// fleet set never changes for the life of the connection needs no refresh
    /// tick, and a caller that DOES need one drives [`FanIn`] itself.
    pub async fn multiplex_of(&self, fleets: &BTreeSet<String>) -> BoxStream<'static, Frame> {
        let mut fan_in = self.fan_in();
        fan_in.sync_to(fleets).await;
        stream::unfold(fan_in, async move |mut fan_in| {
            let frame = fan_in.next_frame().await;
            Some((frame, fan_in))
        })
        .boxed()
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "stream admission and pending futures are the invariants under test"
    )]

    use std::collections::BTreeSet;
    use std::time::Duration;

    use futures_util::StreamExt as _;

    use super::{Live, StreamLimit};

    #[tokio::test]
    async fn detached_live_streams_are_admitted_and_announce_the_gap() {
        let live = Live::detached(StreamLimit::new(2));
        assert!(live.bus().is_none());
        assert_eq!(live.capacity(), 2);
        let slot = live.admit().expect("the first detached stream is admitted");
        assert_eq!(live.carrying(), 1);

        let mut tail = live
            .tail_of(
                &af_core::id::Uuid7::parse("0195b4ba-8d3a-7000-8abc-000000000001")
                    .expect("canonical fleet"),
            )
            .await;
        for kind in ["hello", "catching_up"] {
            let frame = tail.next().await.expect("the detached tail opens");
            assert_eq!(
                frame.kind, kind,
                "a detached tail announces it cannot follow"
            );
        }
        tokio::time::timeout(Duration::from_millis(1), tail.next())
            .await
            .expect_err("then a detached tail retries quietly");

        let mut multiplex = live
            .multiplex_of(&BTreeSet::from(["fleet-a".to_owned()]))
            .await;
        tokio::time::timeout(Duration::from_millis(1), multiplex.next())
            .await
            .expect_err("a detached fan-in must remain pending");
        drop(slot);
        assert_eq!(live.carrying(), 0);
    }
}
