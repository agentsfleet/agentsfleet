//! The live surface, composed: the pub/sub connection and the instance's
//! stream budget, as one value a handler holds.
//!
//! # A deployment with no hub is a value, not a `None` every handler renders
//!
//! `afd_fleet::bundle::Bundles::unconfigured` is the shape this follows. An
//! instance with no pub/sub connection still SERVES the stream routes: the
//! connection opens, the ceiling is charged, the heartbeat runs, and no frame
//! ever arrives. That is the honest behaviour — the events list is still there
//! and a client polls it — where a 500 would take the dashboard down over a
//! surface that is by construction best-effort.

use std::collections::BTreeSet;

use afd_redis::SubscriptionHub;
use futures_util::StreamExt as _;
use futures_util::stream::{self, BoxStream};

use crate::ceiling::{Ceiling, Slot};
use crate::channel;
use crate::fanin::FanIn;
use crate::frame::Frame;
use crate::tail::tail;

/// What the two stream routes act through.
#[derive(Debug, Clone)]
pub struct Live {
    hub: Option<SubscriptionHub>,
    ceiling: Ceiling,
}

impl Live {
    /// The live surface of an instance holding a pub/sub connection.
    #[must_use]
    pub fn new(hub: SubscriptionHub, ceiling: Ceiling) -> Self {
        Self {
            hub: Some(hub),
            ceiling,
        }
    }

    /// The live surface of an instance with no pub/sub connection.
    ///
    /// Its streams open, count against the ceiling, and stay silent. See the
    /// module header for why that is the behaviour rather than a refusal.
    #[must_use]
    pub const fn detached(ceiling: Ceiling) -> Self {
        Self { hub: None, ceiling }
    }

    /// The hub behind this surface, for the shutdown that closes it.
    ///
    /// `None` on a detached instance, which has nothing to close.
    #[must_use]
    pub const fn hub(&self) -> Option<&SubscriptionHub> {
        self.hub.as_ref()
    }

    /// A slot for one stream, or `None` when the instance is full or draining.
    #[must_use]
    pub fn admit(&self) -> Option<Slot> {
        self.ceiling.admit()
    }

    /// How many streams this instance is carrying.
    #[must_use]
    pub fn carrying(&self) -> usize {
        self.ceiling.live()
    }

    /// How many it will carry.
    #[must_use]
    pub const fn capacity(&self) -> usize {
        self.ceiling.capacity()
    }

    /// Every frame published for one fleet, numbered from zero.
    #[must_use]
    pub fn tail_of(&self, fleet_id: &str) -> BoxStream<'static, Frame> {
        match self.hub.as_ref() {
            Some(hub) => tail(hub.subscribe(&channel::activity(fleet_id))).boxed(),
            None => stream::pending().boxed(),
        }
    }

    /// A fan-in the caller attaches a workspace's fleets to.
    #[must_use]
    pub fn fan_in(&self) -> FanIn {
        FanIn::new(self.hub.clone())
    }

    /// The frames a fan-in carrying `fleets` would produce, as one stream.
    ///
    /// The convenience the per-fleet route gets for free: a workspace whose
    /// fleet set never changes for the life of the connection needs no refresh
    /// tick, and a caller that DOES need one drives [`FanIn`] itself.
    #[must_use]
    pub fn multiplex_of(&self, fleets: &BTreeSet<String>) -> BoxStream<'static, Frame> {
        let mut fan_in = self.fan_in();
        fan_in.sync_to(fleets);
        stream::unfold(fan_in, |mut fan_in| async move {
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

    use super::Live;
    use crate::ceiling::Ceiling;

    #[tokio::test]
    async fn detached_live_streams_are_admitted_but_stay_silent() {
        let live = Live::detached(Ceiling::new(2));
        assert!(live.hub().is_none());
        assert_eq!(live.capacity(), 2);
        let slot = live.admit().expect("the first detached stream is admitted");
        assert_eq!(live.carrying(), 1);

        let mut tail = live.tail_of("fleet-a");
        tokio::time::timeout(Duration::from_millis(1), tail.next())
            .await
            .expect_err("a detached tail must remain pending");

        let mut multiplex = live.multiplex_of(&BTreeSet::from(["fleet-a".to_owned()]));
        tokio::time::timeout(Duration::from_millis(1), multiplex.next())
            .await
            .expect_err("a detached fan-in must remain pending");
        drop(slot);
        assert_eq!(live.carrying(), 0);
    }
}
