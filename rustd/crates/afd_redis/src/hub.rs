//! One pub/sub connection per process, fanned out to every reader.
//!
//! # Why the connection is shared and the channels are not
//!
//! Pub/sub takes a connection over: once `SUBSCRIBE` is issued the server
//! pushes messages down that socket and no ordinary command may share it. The
//! naive shape — a connection per reader — makes a browser tab a socket, and a
//! few hundred open event streams a few hundred Redis connections.
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
//! Redis restarts, failovers and idle timeouts all end the socket. The pump
//! reconnects with jittered backoff and resubscribes everything still
//! referenced, so readers keep their receivers across the gap and see messages
//! resume rather than an error. What they lose is what was published while the
//! socket was down; pub/sub has no replay, and pretending otherwise would be
//! the lie. `test_hub_reconnect_resubscribes` holds that.

mod pump;

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::{broadcast, mpsc};

use crate::config::RedisConfig;
use crate::error::{Error, ErrorKind};

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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Backoff {
    initial: Duration,
    max: Duration,
}

impl Backoff {
    /// The production schedule: a fifth of a second, doubling to five.
    pub const PRODUCTION: Self = Self {
        initial: Duration::from_millis(200),
        max: Duration::from_secs(5),
    };

    /// A schedule a test can wait out.
    #[must_use]
    pub const fn new(initial: Duration, max: Duration) -> Self {
        Self { initial, max }
    }

    /// The delay after `attempt` consecutive failures, doubling and capped.
    ///
    /// Jitter is deliberate: without it every process that lost the same Redis
    /// redials in the same millisecond, and the reconnect storm is what keeps
    /// it down. Public because the schedule is a promise to whoever runs this —
    /// how long an outage takes to recover from is an operational number, not
    /// an implementation detail.
    #[must_use]
    pub fn delay(self, attempt: u32, jitter: u64) -> Duration {
        let doubled = self
            .initial
            .saturating_mul(2_u32.saturating_pow(attempt.min(16)));
        let capped = doubled.min(self.max);
        let spread = u64::try_from(capped.as_millis() / 4).unwrap_or(0);
        capped + Duration::from_millis(if spread == 0 { 0 } else { jitter % spread })
    }
}

/// A live subscription. Dropping it releases the caller's interest in the
/// channel, and the last drop unsubscribes it server-side.
#[derive(Debug)]
pub struct Subscription {
    channel: String,
    receiver: broadcast::Receiver<Message>,
    hub: Arc<HubInner>,
}

impl Subscription {
    /// The channel this subscription reads.
    #[must_use]
    pub fn channel(&self) -> &str {
        &self.channel
    }

    /// Waits for the next message.
    ///
    /// # Errors
    /// Returns a hub-closed error once the hub is shut down. A reader that fell
    /// behind the buffer is told so by `Ok(None)` — see [`Lagged`].
    ///
    /// [`Lagged`]: tokio::sync::broadcast::error::RecvError::Lagged
    pub async fn recv(&mut self) -> Result<Option<Message>, Error> {
        match self.receiver.recv().await {
            Ok(message) => Ok(Some(message)),
            Err(broadcast::error::RecvError::Lagged(missed)) => {
                tracing::warn!(
                    channel = self.channel,
                    missed,
                    error_code = afd_core::error_code::INTERNAL_OPERATION_FAILED.as_str(),
                    "hub_subscriber_lagged"
                );
                Ok(None)
            }
            Err(broadcast::error::RecvError::Closed) => Err(Error::new(ErrorKind::HubClosed)),
        }
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        self.hub.release(&self.channel);
    }
}

/// The process's single pub/sub connection, and the channels riding it.
#[derive(Debug, Clone)]
pub struct SubscriptionHub {
    inner: Arc<HubInner>,
}

#[derive(Debug)]
pub(crate) struct HubInner {
    channels: Mutex<HashMap<String, ChannelEntry>>,
    commands: mpsc::UnboundedSender<Command>,
    /// How many times a connection has been established, including the first.
    /// A process that opens two has broken Invariant 2, and this is how a test
    /// sees it without counting sockets on the server.
    connections_opened: AtomicU64,
}

#[derive(Debug)]
pub(crate) struct ChannelEntry {
    sender: broadcast::Sender<Message>,
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
    pub async fn start(config: RedisConfig) -> Result<Self, Error> {
        Self::start_with_backoff(config, Backoff::PRODUCTION).await
    }

    /// Starts the hub with a reconnect schedule of the caller's choosing.
    ///
    /// # Errors
    /// As [`SubscriptionHub::start`].
    pub async fn start_with_backoff(config: RedisConfig, backoff: Backoff) -> Result<Self, Error> {
        let (commands, receiver) = mpsc::unbounded_channel();
        let inner = Arc::new(HubInner {
            channels: Mutex::new(HashMap::new()),
            commands,
            connections_opened: AtomicU64::new(0),
        });

        pump::spawn(config, backoff, Arc::clone(&inner), receiver).await?;
        Ok(Self { inner })
    }

    /// Subscribes to `channel`, sharing the connection with every other reader.
    ///
    /// The server-side `SUBSCRIBE` is issued only for the first reader of a
    /// channel; the rest are handed a receiver on the same broadcast.
    #[must_use]
    pub fn subscribe(&self, channel: &str) -> Subscription {
        let receiver = {
            let mut channels = self.inner.lock_channels();
            if let Some(entry) = channels.get_mut(channel) {
                entry.readers += 1;
                entry.sender.subscribe()
            } else {
                let (sender, receiver) = broadcast::channel(CHANNEL_CAPACITY);
                channels.insert(channel.to_owned(), ChannelEntry { sender, readers: 1 });
                // Sent while holding the lock on purpose: the pump must not see
                // an Unsubscribe for a channel whose Subscribe has not been
                // queued yet, and the lock is what orders them.
                let _ = self
                    .inner
                    .commands
                    .send(Command::Subscribe(channel.to_owned()));
                receiver
            }
        };

        Subscription {
            channel: channel.to_owned(),
            receiver,
            hub: Arc::clone(&self.inner),
        }
    }

    /// How many readers hold a subscription to `channel`.
    #[must_use]
    pub fn readers(&self, channel: &str) -> usize {
        self.inner
            .lock_channels()
            .get(channel)
            .map_or(0, |entry| entry.readers)
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
    /// The channel map. Poisoning cannot happen — nothing that runs under this
    /// lock can panic — and recovering the guard is the honest answer if it
    /// somehow did, rather than propagating a panic into every later caller.
    pub(crate) fn lock_channels(&self) -> std::sync::MutexGuard<'_, HashMap<String, ChannelEntry>> {
        self.channels
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Every channel with at least one reader, for a resubscribe after a drop.
    pub(crate) fn live_channels(&self) -> Vec<String> {
        self.lock_channels().keys().cloned().collect()
    }

    /// Hands a message to the readers of its channel.
    pub(crate) fn dispatch(&self, message: Message) {
        if let Some(entry) = self.lock_channels().get(&message.channel) {
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
    fn release(&self, channel: &str) {
        let mut channels = self.lock_channels();
        let Some(entry) = channels.get_mut(channel) else {
            return;
        };
        entry.readers = entry.readers.saturating_sub(1);
        if entry.readers == 0 {
            channels.remove(channel);
            let _ = self.commands.send(Command::Unsubscribe(channel.to_owned()));
        }
    }
}
