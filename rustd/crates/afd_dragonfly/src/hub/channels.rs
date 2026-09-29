//! The channel table every reader and both hub tasks share.
//!
//! Split from `hub.rs` along the refcount seam: `hub.rs` is what a reader
//! holds and sees, this is the per-channel state behind it and the ordering
//! rule that keeps a channel's `Subscribe` ahead of its `Unsubscribe`.
//!
//! # A confirmation after a channel's first is a gap
//!
//! Every `SSUBSCRIBE` the server accepts is confirmed with a push, and the
//! hub issues exactly one per channel for as long as a reader holds it. A
//! SECOND confirmation therefore means the subscription was lost and
//! re-issued — by the driver repairing a node, by the hub after a slot moved,
//! or by a redial — and every frame published in between is gone. Readers
//! are told at that moment, after the subscription exists again, so the
//! backfill they run cannot race frames still in flight on the old one.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use dashmap::DashMap;
use dashmap::mapref::entry::Entry;
use tokio::sync::{broadcast, mpsc};

use super::Message;
#[cfg(doc)]
use super::{Subscription, SubscriptionHub};

/// The shared state, and deliberately NOT the command sender.
///
/// Both hub tasks hold an `Arc<HubInner>` for as long as they run. If the
/// sender lived here, that `Arc` would keep it alive, `commands.recv()` could
/// never return `None`, and the control task could never learn that the last
/// handle had gone — a task that pumps a live Dragonfly socket forever with no
/// way to stop it, and no stop path for §7's supervisor to join. The sender
/// therefore lives with the handles that represent a caller's interest:
/// [`SubscriptionHub`] and [`Subscription`]. When the last of those drops, the
/// channel closes, the control task returns and takes the dispatch task with
/// it.
#[derive(Debug)]
pub(crate) struct HubInner {
    /// One entry per channel some reader holds.
    ///
    /// Sharded rather than one map behind one lock, because every frame this
    /// hub receives looks its channel up here: a deployment streaming a
    /// hundred fleets put every one of those dispatches through a single
    /// lock, behind every subscribe and release as well. What the shape has
    /// to preserve is the ORDERING of a channel's own commands — see
    /// [`SubscriptionHub::subscribe`] and [`HubInner::release`], which take
    /// and hold that channel's entry across the send — and per-key is exactly
    /// what a sharded map gives. Two DIFFERENT channels never needed ordering
    /// between them; they are independent subscriptions on one socket.
    pub(super) channels: DashMap<String, ChannelEntry>,
    /// How many times a connection has been established, including the first.
    /// A process that opens two has broken Invariant 2, and this is how a test
    /// sees it without counting sockets on the server.
    pub(super) connections_opened: AtomicU64,
}

#[derive(Debug)]
pub(crate) struct ChannelEntry {
    pub(super) sender: broadcast::Sender<Delivery>,
    /// How many [`Subscription`]s this channel is being held open by.
    ///
    /// # Not `sender.receiver_count()`, and this is load-bearing
    ///
    /// `broadcast::Sender` already counts its receivers, so this field reads
    /// like a duplicate of one. It is not, because of WHEN it is read:
    /// [`HubInner::release`] runs from `Subscription`'s `Drop`, and Rust runs a
    /// type's `drop` before dropping its fields — so the receiver belonging to
    /// the subscription being released is still alive at that moment.
    /// `receiver_count()` there answers 1 for the last reader leaving, never 0,
    /// and the unsubscribe condition would have to be spelled `== 1` with a
    /// comment explaining that 1 means none.
    ///
    /// The deeper reason is that these two numbers answer different questions.
    /// `receiver_count()` observes how many receivers exist. This counts how
    /// many callers have declared an interest, and it is what decides whether
    /// the server is told to `UNSUBSCRIBE` — a lifecycle decision this hub
    /// owns, which should not be inferred from a tokio internal that is free
    /// to change what it counts.
    pub(super) readers: usize,
    /// Whether the server has confirmed this channel's subscription once.
    /// The next confirmation is a gap; see the module header.
    pub(super) confirmed: bool,
}

/// What a channel's readers are handed.
///
/// The frame is behind an `Arc` because `broadcast` clones an item once per
/// receiver: a `Message` by value was a payload copy per viewer, which is the
/// cost that grew with an audience rather than with traffic.
#[derive(Debug, Clone)]
pub(crate) enum Delivery {
    /// One frame, shared by every reader of its channel.
    Message(Arc<Message>),
    /// The subscription was lost and is back; frames in between are gone.
    Gap,
}

/// What one subscribe confirmation meant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Confirmation {
    /// The channel's first: its subscription exists now.
    First,
    /// A later one: its readers were sent a [`Delivery::Gap`].
    Replay,
    /// No reader holds the channel any more, so there was nobody to tell.
    Unheld,
}

/// What the control task is asked to do with the socket it owns.
#[derive(Debug)]
pub(crate) enum Command {
    Subscribe(String),
    Unsubscribe(String),
}

impl HubInner {
    /// An empty table that has opened no connection yet.
    pub(super) fn new() -> Self {
        Self {
            channels: DashMap::new(),
            connections_opened: AtomicU64::new(0),
        }
    }

    /// Every channel with at least one reader, for a resubscribe after a drop.
    ///
    /// The walk locks one shard at a time, so a channel subscribed while it
    /// runs may land on either side of it. Nothing is stranded by that: a
    /// subscribe queues its own `Subscribe` command while holding the
    /// channel's entry, and the pump drains that queue as soon as it has
    /// resubscribed what this returned — so a channel this misses is
    /// subscribed by its own command a moment later, and one it catches twice
    /// is an `SSUBSCRIBE` the server already answers idempotently.
    pub(crate) fn live_channels(&self) -> Vec<String> {
        self.channels
            .iter()
            .map(|entry| entry.key().clone())
            .collect()
    }

    /// Whether any reader still holds `channel`.
    ///
    /// One key rather than [`HubInner::live_channels`] and a scan of it: this
    /// is asked on every `sunsubscribe` push the server sends, and cloning
    /// every channel name to answer a question about one of them was a cost
    /// that grew with the deployment.
    pub(crate) fn holds_channel(&self, channel: &str) -> bool {
        self.channels.contains_key(channel)
    }

    /// Hands a message to the readers of its channel, allocated once for all
    /// of them.
    pub(crate) fn dispatch(&self, message: Message) {
        if let Some(entry) = self.channels.get(&message.channel) {
            // The error case is "no receivers right now", which is not a
            // failure: a subscription being dropped as a message arrives is an
            // ordinary race, and the refcount cleanup is already on its way.
            let _ = entry.sender.send(Delivery::Message(Arc::new(message)));
        }
    }

    /// Records the server confirming `channel`'s subscription, and tells its
    /// readers about the gap when this is not the first confirmation.
    ///
    /// The entry is held for writing across the send, so a reader joining
    /// the channel at that moment either receives the gap or subscribes after
    /// it — never half of a state change.
    pub(crate) fn confirm(&self, channel: &str) -> Confirmation {
        let Some(mut entry) = self.channels.get_mut(channel) else {
            return Confirmation::Unheld;
        };
        if !entry.confirmed {
            entry.confirmed = true;
            return Confirmation::First;
        }
        let _ = entry.sender.send(Delivery::Gap);
        Confirmation::Replay
    }

    pub(crate) fn record_connection(&self) {
        self.connections_opened.fetch_add(1, Ordering::AcqRel);
    }

    /// Drops one reader's interest, unsubscribing when the last one goes.
    ///
    /// The sender arrives as an argument rather than as a field, and the send
    /// happens while the channel map is still locked. That ordering is the
    /// point: an `Unsubscribe` that overtook the `Subscribe` of a reader
    /// arriving on the same channel would leave that reader holding a live
    /// subscription the server had been told to drop.
    pub(super) fn release(&self, channel: &str, commands: &mpsc::UnboundedSender<Command>) {
        let Entry::Occupied(mut held) = self.channels.entry(channel.to_owned()) else {
            return;
        };
        let entry = held.get_mut();
        entry.readers = entry.readers.saturating_sub(1);
        if entry.readers == 0 {
            // Queued BEFORE the entry is removed, so the send still happens
            // while this channel's shard is held: a reader arriving on this
            // channel blocks on that entry, and its Subscribe therefore
            // queues after this Unsubscribe rather than being overtaken by
            // it. `remove` consumes the entry and releases the shard, so the
            // two cannot be written the other way round.
            let _ = commands.send(Command::Unsubscribe(channel.to_owned()));
            held.remove();
        }
    }
}
