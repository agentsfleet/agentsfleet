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
//! Dragonfly restarts, failovers and idle timeouts all end a socket. The
//! cluster driver keeps one per node and repairs a node's on its own; the hub
//! then re-subscribes every channel on the connection it has, leaving every
//! other node's frames flowing. A loss that is not confirmed back inside the
//! repair window is redialled whole with jittered backoff. Either way readers keep
//! their receivers and see messages resume rather than an error. What they
//! lose is what was published while their subscription was down — pub/sub
//! has no replay — and they are TOLD: [`Received::Gap`] arrives once the
//! subscription is back, so a backfill from the durable log closes the hole.
//! `test_node_loss_is_a_gap_not_a_reconnect` and `test_reconnect_sends_a_gap`
//! hold that.
//!
//! # Two tasks, so a subscribe never holds a frame
//!
//! The control task owns the connection's commands and its redial; the
//! dispatch task owns the pushes. A subscribe is a round trip, and one slow
//! node answering it used to park every frame for every channel behind it.

mod channels;
#[cfg(feature = "test-util")]
mod detached;
mod dispatch;
mod gap;
mod pump;
mod repair;

#[cfg(test)]
mod tests;

#[cfg(feature = "test-util")]
pub(crate) use gap::NODE_REPAIR_WINDOW;

#[cfg(feature = "test-util")]
pub use self::detached::Server as DetachedServer;

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use backon::ExponentialBuilder;
use dashmap::mapref::entry::Entry;
use tokio::sync::{broadcast, mpsc};

use self::channels::{ChannelEntry, Command, Delivery, HubInner};
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
    receiver: broadcast::Receiver<Delivery>,
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
            Ok(Delivery::Message(message)) => Ok(Received::Message(message)),
            Ok(Delivery::Gap) => Ok(Received::Gap),
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
///
/// The gap arm carries no count because nobody has one: the frames it covers
/// were published while the server held no subscription for this channel, so
/// no process ever saw them.
///
/// Exhaustive on purpose, where most of this workspace's public enums are not:
/// a wait either produced a message or reported what it missed, and a reader
/// that forwards frames must handle every way of missing them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Received {
    /// One message, as published, shared with every other reader of the
    /// channel rather than copied for each.
    Message(Arc<Message>),
    /// The reader fell behind and this many messages were dropped for it.
    Lagged(u64),
    /// The channel's subscription was lost and is back. Frames published in
    /// between are gone, and the count of them is unknowable.
    Gap,
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
        let inner = Arc::new(HubInner::new());

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
                let held = slot.insert(ChannelEntry {
                    sender,
                    readers: 1,
                    confirmed: false,
                });
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
    /// One, unless it has had to redial. Never one per subscriber — that is
    /// Invariant 2, and this is the number that proves it — and never one per
    /// node repair, which the driver performs inside the connection it has.
    #[must_use]
    pub fn connections_opened(&self) -> u64 {
        self.inner.connections_opened.load(Ordering::Acquire)
    }
}
