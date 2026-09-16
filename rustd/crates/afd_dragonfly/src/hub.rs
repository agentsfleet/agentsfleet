//! One pub/sub connection per process, fanned out to every reader.
//!
//! # Why the connection is shared and the channels are not
//!
//! Pub/sub takes a connection over: once `SUBSCRIBE` is issued the server
//! pushes messages down that socket and no ordinary command may share it. The
//! naive shape — a connection per reader — makes a browser tab a socket, and a
//! few hundred open event streams a few hundred Dragonfly connections.
//!
//! So the hub owns exactly ONE and multiplexes locally: a channel is subscribed
//! server-side the first time anybody asks for it, every later asker gets a
//! [`tokio::sync::broadcast`] receiver on the same channel, and the server-side
//! subscription is dropped when the last reader goes away. That refcount is
//! [`Subscription`]'s `Drop`, not a method anyone has to remember to call.
//!
//! Invariant 2 of the milestone — exactly one subscribe connection per process
//! — is what this type is for, and `test_hub_refcount_single_connection` is
//! what holds it.
//!
//! # A dropped connection is expected, not exceptional
//!
//! Dragonfly restarts, failovers and idle timeouts all end the socket. The pump
//! reconnects with jittered backoff and resubscribes everything still
//! referenced, so readers keep their receivers across the gap and see messages
//! resume rather than an error. What they lose is what was published while the
//! socket was down; pub/sub has no replay, and pretending otherwise would be
//! the lie. `test_hub_reconnect_resubscribes` holds that.

mod pump;

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use backon::ExponentialBuilder;
use dashmap::DashMap;
use dashmap::mapref::entry::Entry;
use tokio::sync::{broadcast, mpsc};

use crate::config::DragonflyConfig;
use crate::error::{Error, ErrorKind, Result};

/// How many messages a slow reader may fall behind before it is told it lagged.
///
/// Bounded on purpose: an unbounded buffer turns one stalled browser tab into
/// the process's memory ceiling. A reader that falls this far behind is told
/// which is honest — the alternative is silently dropping messages it believes
/// it received.
const CHANNEL_CAPACITY: usize = 256;

/// One published message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    /// The channel it arrived on.
    pub channel: String,
    /// The payload, as published.
    pub payload: String,
}

/// How long the pump waits before redialling a dropped connection.
///
/// A schedule is an operational promise — how long an outage takes to recover
/// from — so it is named here rather than buried in the pump, and it is
/// `backon`'s builder rather than a type of ours. Jitter is on: without it
/// every process that lost the same Dragonfly redials in the same millisecond, and
/// the reconnect storm is what keeps it down.
///
/// There is no attempt limit, and that is the pub/sub contract rather than an
/// oversight: a reader holds a receiver, not a connection, so there is nobody
/// to hand a give-up to and nothing sensible to do with one but try again.
/// `without_max_times` overrides `backon`'s default of three.
///
/// Jitter here ADDS to the step rather than replacing it — `backon` offsets by
/// a random amount inside `(0, current_delay)` — so a delay lands in
/// `[step, 2 × step)` and the ceiling below is on the step, not on the wait.
#[must_use]
pub const fn production_backoff() -> ExponentialBuilder {
    // A fifth of a second, doubling to five — the schedule this hub has always
    // redialled on.
    ExponentialBuilder::new()
        .with_min_delay(Duration::from_millis(200))
        .with_max_delay(Duration::from_secs(5))
        .without_max_times()
        .with_jitter()
}

/// A live subscription. Dropping it releases the caller's interest in the
/// channel, and the last drop unsubscribes it server-side.
#[derive(Debug)]
pub struct Subscription {
    channel: String,
    receiver: broadcast::Receiver<Message>,
    hub: Arc<HubInner>,
    /// This reader's handle on the pump. Held here rather than reached through
    /// `hub` for a lifetime reason, not a convenience one — see [`HubInner`].
    commands: mpsc::UnboundedSender<Command>,
}

impl Subscription {
    /// The channel this subscription reads.
    #[must_use]
    pub fn channel(&self) -> &str {
        &self.channel
    }

    /// Waits for the next message, or for the news that some were missed.
    ///
    /// # Errors
    /// Returns a hub-closed error once the hub is shut down.
    pub async fn recv(&mut self) -> Result<Received> {
        match self.receiver.recv().await {
            Ok(message) => Ok(Received::Message(message)),
            Err(broadcast::error::RecvError::Lagged(missed)) => {
                // Hoisted: see the `tracing` note in the workspace Cargo.toml.
                let error_code = afd_core::error_code::INTERNAL_OPERATION_FAILED.as_str();
                tracing::warn!(
                    channel = self.channel,
                    missed,
                    error_code,
                    event = "hub_subscriber_lagged"
                );
                Ok(Received::Lagged(missed))
            }
            Err(broadcast::error::RecvError::Closed) => Err(Error::new(ErrorKind::HubClosed)),
        }
    }
}

/// What one wait on a subscription produced.
///
/// The lag arm carries its COUNT rather than being a bare "you missed some".
/// A reader that forwards frames to a person owes them the number — the live
/// stream says "catching up, 12 dropped", and a boolean could only have said
/// that something went wrong. The buffer is per reader, so a slow one is told
/// about its own backlog and a fast one on the same channel is unaffected.
/// Exhaustive on purpose, where most of this workspace's public enums are not:
/// a wait either produced a message or reported what it missed, and there is no
/// third answer a later version could add. Marking it `non_exhaustive` would
/// cost every reader an arm it can never reach.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Received {
    /// One message, as published.
    Message(Message),
    /// The reader fell behind and this many messages were dropped for it.
    Lagged(u64),
}

impl Drop for Subscription {
    fn drop(&mut self) {
        self.hub.release(&self.channel, &self.commands);
    }
}

/// The process's single pub/sub connection, and the channels riding it.
#[derive(Debug, Clone)]
pub struct SubscriptionHub {
    inner: Arc<HubInner>,
    commands: mpsc::UnboundedSender<Command>,
}

/// The shared state, and deliberately NOT the command sender.
///
/// The pump task holds an `Arc<HubInner>` for as long as it runs. If the sender
/// lived here, that `Arc` would keep it alive, `commands.recv()` could never
/// return `None`, and the pump could never learn that the last handle had gone
/// — a task that pumps a live Dragonfly socket forever with no way to stop it, and
/// no stop path for §7's supervisor to join. The sender therefore lives with
/// the handles that represent a caller's interest: [`SubscriptionHub`] and
/// [`Subscription`]. When the last of those drops, the channel closes and the
/// pump returns.
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
    channels: DashMap<String, ChannelEntry>,
    /// How many times a connection has been established, including the first.
    /// A process that opens two has broken Invariant 2, and this is how a test
    /// sees it without counting sockets on the server.
    connections_opened: AtomicU64,
}

#[derive(Debug)]
pub(crate) struct ChannelEntry {
    sender: broadcast::Sender<Message>,
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
    readers: usize,
}

/// What the pump is asked to do with the socket it owns.
#[derive(Debug)]
pub(crate) enum Command {
    Subscribe(String),
    Unsubscribe(String),
}

impl SubscriptionHub {
    /// Starts the hub, opening its one connection.
    ///
    /// # Errors
    /// Returns an unavailable error when the first connection cannot be made.
    /// Later drops are the pump's problem, not the caller's.
    pub async fn start(config: DragonflyConfig) -> Result<Self> {
        Self::start_with_backoff(config, production_backoff()).await
    }

    /// Starts the hub with a reconnect schedule of the caller's choosing.
    ///
    /// # Errors
    /// As [`SubscriptionHub::start`].
    pub async fn start_with_backoff(
        config: DragonflyConfig,
        schedule: ExponentialBuilder,
    ) -> Result<Self> {
        let (commands, receiver) = mpsc::unbounded_channel();
        let inner = Arc::new(HubInner {
            channels: DashMap::new(),
            connections_opened: AtomicU64::new(0),
        });

        pump::spawn(config, schedule, Arc::clone(&inner), receiver).await?;
        Ok(Self { inner, commands })
    }

    /// Subscribes to `channel`, sharing the connection with every other reader.
    ///
    /// The server-side `SUBSCRIBE` is issued only for the first reader of a
    /// channel; the rest are handed a receiver on the same broadcast.
    #[must_use]
    pub fn subscribe(&self, channel: &str) -> Subscription {
        let receiver = match self.inner.channels.entry(channel.to_owned()) {
            Entry::Occupied(mut held) => {
                let entry = held.get_mut();
                entry.readers += 1;
                entry.sender.subscribe()
            }
            Entry::Vacant(slot) => {
                let (sender, receiver) = broadcast::channel(CHANNEL_CAPACITY);
                // The guard is BOUND rather than dropped, because the send
                // below has to happen while this channel's entry is still
                // held: the pump must not see an Unsubscribe for a channel
                // whose Subscribe has not been queued yet, and holding the
                // entry is what orders them. Per channel is the whole
                // requirement — two channels' commands were never ordered
                // against each other.
                let held = slot.insert(ChannelEntry { sender, readers: 1 });
                let _ = self.commands.send(Command::Subscribe(channel.to_owned()));
                drop(held);
                receiver
            }
        };

        Subscription {
            channel: channel.to_owned(),
            receiver,
            hub: Arc::clone(&self.inner),
            commands: self.commands.clone(),
        }
    }

    /// How many readers hold a subscription to `channel`.
    #[must_use]
    pub fn readers(&self, channel: &str) -> usize {
        self.inner
            .channels
            .get(channel)
            .map_or(0, |entry| entry.readers)
    }

    /// Drops every channel, closing what readers are waiting on.
    ///
    /// §7's supervisor calls this in stop order: a process that is going away
    /// must tell its readers so, rather than leaving them parked on a socket
    /// nobody is pumping. A reader waiting on a closed channel gets a
    /// hub-closed error, which is a thing it can act on; a reader waiting on an
    /// abandoned one waits forever.
    pub fn shutdown(&self) {
        self.inner.channels.clear();
    }

    /// How many connections this hub has opened over its life.
    ///
    /// One, unless it has had to reconnect. Never one per subscriber — that is
    /// Invariant 2, and this is the number that proves it.
    #[must_use]
    pub fn connections_opened(&self) -> u64 {
        self.inner.connections_opened.load(Ordering::Acquire)
    }
}

impl HubInner {
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

    /// Hands a message to the readers of its channel.
    pub(crate) fn dispatch(&self, message: Message) {
        if let Some(entry) = self.channels.get(&message.channel) {
            // The error case is "no receivers right now", which is not a
            // failure: a subscription being dropped as a message arrives is an
            // ordinary race, and the refcount cleanup is already on its way.
            let _ = entry.sender.send(message);
        }
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
    fn release(&self, channel: &str, commands: &mpsc::UnboundedSender<Command>) {
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
