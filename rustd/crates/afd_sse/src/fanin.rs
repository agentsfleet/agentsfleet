//! Many fleets' activity on one connection.
//!
//! The wall used to open a stream per tile, so L live fleets watched by V
//! operators cost L×V connections and L×V slots. This carries the whole
//! workspace on one.
//!
//! # Isolation is by construction, never by filtering
//!
//! Only the fleets the caller may read are ever subscribed, so a frame from
//! another workspace is not delivered and then discarded — it never arrives.
//! There is deliberately no pattern subscribe: `PSUBSCRIBE fleet:*` would put
//! every tenant's frames on one firehose and make tenant isolation a matter of
//! discipline rather than of what is attached.
//!
//! # The set changes while the connection is open
//!
//! A fleet installed after a tab was opened has to appear in it. The caller
//! ticks [`FanIn::sync_to`] with the fleets it currently authorizes, and this
//! attaches and detaches to match. Detaching cancels that channel's own stream,
//! which drops its subscription — so the hub unsubscribes when the last viewer
//! of a channel goes, with no bookkeeping here.
//!
//! # What this does NOT do
//!
//! Enumerate fleets, and re-authorize the caller. Both are reads against a
//! workspace, both belong to the handler that holds the pool, and both are
//! per-tick decisions this type is TOLD the answer to.

use std::collections::{BTreeMap, BTreeSet};

use afd_redis::hub::Received;
use afd_redis::{Subscription, SubscriptionHub};
use futures_util::StreamExt as _;
use futures_util::stream::{self, BoxStream, SelectAll};
use tokio_util::sync::CancellationToken;

use crate::channel;
use crate::error::Error;
use crate::frame::Frame;

/// One arrival on the shared consumer, before it becomes a frame.
#[derive(Debug)]
enum Arrival {
    /// A payload published on one fleet's channel.
    Published { fleet_id: String, payload: String },
    /// This connection fell behind and missed messages on one channel.
    Missed(u64),
}

/// What one [`FanIn::sync_to`] changed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Delta {
    /// Channels attached this tick.
    pub attached: usize,
    /// Channels detached this tick.
    pub detached: usize,
}

impl Delta {
    /// Whether the attached set moved at all.
    #[must_use]
    pub const fn is_change(self) -> bool {
        self.attached > 0 || self.detached > 0
    }
}

/// One frame and the fleet it came from, as the caller sees it.
pub type Tagged = Frame;

/// The channels behind one workspace stream.
pub struct FanIn {
    /// `None` on a deployment with no pub/sub connection. The connection still
    /// opens and still heartbeats — it simply carries no channels — because a
    /// stream surface that 500s when the queue is absent would take the whole
    /// dashboard down with it.
    hub: Option<SubscriptionHub>,
    /// The fleets currently attached, and the token that detaches each.
    ///
    /// A map rather than a `Vec` of pairs: `sync_to` asks "is this one already
    /// attached?" once per wanted fleet, which over a wall of tiles is a linear
    /// scan per tile. It also keeps [`fleets`](FanIn::fleets) free — the client
    /// is announced a LIST, and a set whose order changed between ticks would
    /// read to it as a set that changed, so the order has to be stable either
    /// way and a `BTreeMap` is already in it.
    attached: BTreeMap<String, CancellationToken>,
    /// Every attached channel, polled as one stream.
    arrivals: SelectAll<BoxStream<'static, Arrival>>,
    /// This CONNECTION's counter — not any channel's.
    seq: u64,
}

impl std::fmt::Debug for FanIn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FanIn")
            .field("attached", &self.attached.len())
            .field("seq", &self.seq)
            .finish_non_exhaustive()
    }
}

impl FanIn {
    /// A fan-in carrying nothing yet.
    ///
    /// Nothing is subscribed until the first [`sync_to`]: a stream that is
    /// refused before it starts costs no wire traffic.
    ///
    /// [`sync_to`]: FanIn::sync_to
    #[must_use]
    pub fn new(hub: Option<SubscriptionHub>) -> Self {
        Self {
            hub,
            attached: BTreeMap::new(),
            arrivals: SelectAll::new(),
            seq: 0,
        }
    }

    /// The fleets currently carried, in a stable order.
    #[must_use]
    pub fn fleets(&self) -> Vec<String> {
        self.attached.keys().cloned().collect()
    }

    /// Aligns the attached channels with `wanted`.
    pub fn sync_to(&mut self, wanted: &BTreeSet<String>) -> Delta {
        let detached = self.detach_absent(wanted);
        let attached = self.attach_missing(wanted);
        Delta { attached, detached }
    }

    /// Detaches every attached fleet no longer in `wanted`.
    fn detach_absent(&mut self, wanted: &BTreeSet<String>) -> usize {
        let before = self.attached.len();
        self.attached.retain(|fleet_id, token| {
            let keep = wanted.contains(fleet_id);
            if !keep {
                // Ends that channel's stream, which drops its subscription —
                // the hub unsubscribes when its last viewer goes.
                token.cancel();
            }
            keep
        });
        before - self.attached.len()
    }

    /// Attaches every fleet in `wanted` that is not attached yet.
    fn attach_missing(&mut self, wanted: &BTreeSet<String>) -> usize {
        let Some(hub) = self.hub.clone() else {
            return 0;
        };
        let mut added = 0;
        for fleet_id in wanted {
            if self.attached.contains_key(fleet_id) {
                continue;
            }
            let token = CancellationToken::new();
            let subscription = hub.subscribe(&channel::activity(fleet_id));
            self.arrivals
                .push(arrivals(subscription, fleet_id.clone(), token.clone()).boxed());
            self.attached.insert(fleet_id.clone(), token);
            added += 1;
        }
        added
    }

    /// The next frame this connection should send.
    ///
    /// Never returns: a workspace whose fleets have all been detached still has
    /// a live connection, and a stream that ended here would close a tab whose
    /// workspace merely has nothing running. The caller races this against its
    /// refresh tick and the heartbeat.
    pub async fn next_frame(&mut self) -> Tagged {
        loop {
            let Some(arrival) = self.arrivals.next().await else {
                // Nothing attached. Park until the caller's next `sync_to`
                // gives us a channel; polling an empty set would spin.
                std::future::pending::<()>().await;
                continue;
            };
            match arrival {
                // A control frame, so it spends no sequence number.
                Arrival::Missed(missed) => return Frame::catching_up(missed),
                Arrival::Published { fleet_id, payload } => {
                    match Frame::tagged(self.seq, &fleet_id, &payload) {
                        Ok(frame) => {
                            self.seq = self.seq.wrapping_add(1);
                            return frame;
                        }
                        // Publisher shape drift. Dropped rather than guessed:
                        // routing it to the wrong tile is worse than losing it.
                        Err(Error::Untaggable) => {
                            let reason = Error::Untaggable.to_string();
                            tracing::debug!(fleet_id, reason, event = "sse_fanin_frame_dropped");
                        }
                    }
                }
            }
        }
    }
}

/// One channel's arrivals, ending when `token` is cancelled or the hub closes.
fn arrivals(
    subscription: Subscription,
    fleet_id: String,
    token: CancellationToken,
) -> impl stream::Stream<Item = Arrival> + Send {
    stream::unfold(
        (subscription, fleet_id, token),
        |(mut subscription, fleet_id, token)| async move {
            let received = tokio::select! {
                // Detached by a refresh tick: end this stream, which is what
                // drops the subscription behind it.
                () = token.cancelled() => return None,
                received = subscription.recv() => received,
            };
            match received {
                Ok(Received::Message(message)) => Some((
                    Arrival::Published {
                        fleet_id: fleet_id.clone(),
                        payload: message.payload,
                    },
                    (subscription, fleet_id, token),
                )),
                Ok(Received::Lagged(missed)) => {
                    Some((Arrival::Missed(missed), (subscription, fleet_id, token)))
                }
                Err(_closed) => None,
            }
        },
    )
}
