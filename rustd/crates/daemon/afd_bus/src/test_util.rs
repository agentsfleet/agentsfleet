//! An in-process Bus for the suites that need fan-out without a broker.
//!
//! Every subject is one broadcast channel: readers share each payload, a reader
//! that falls behind is told how many it lost, and the fixture's server half
//! injects messages and gaps the way a broker and its reconnects would.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use async_trait::async_trait;
use futures_util::stream;
use tokio::sync::broadcast;

use crate::{Bus, Handle, Message, Received, Result, Subject, Subscription, error::ErrorKind};

/// Messages one reader may fall behind before it is told how many it lost.
const CAPACITY: usize = 256;

/// Every subject's channel, keyed by its broker spelling, and whether the bus
/// has shut down.
#[derive(Debug, Default)]
struct Hub {
    channels: Mutex<HashMap<String, broadcast::Sender<Received>>>,
    closed: AtomicBool,
}

impl Hub {
    fn channels(&self) -> MutexGuard<'_, HashMap<String, broadcast::Sender<Received>>> {
        // Each critical section leaves the map whole, so a panic elsewhere
        // cannot leave it half-written.
        self.channels.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn sender(&self, subject: &Subject) -> broadcast::Sender<Received> {
        self.channels()
            .entry(crate::nats::spelling(subject))
            .or_insert_with(|| broadcast::channel(CAPACITY).0)
            .clone()
    }

    /// Hand `received` to every reader of `subject`. No reader is not an
    /// error: the bus is at most once.
    fn send(&self, subject: &Subject, received: Received) {
        let _ = self.sender(subject).send(received);
    }

    fn deliver(&self, subject: &Subject, payload: &str) {
        let message = Message {
            subject: subject.clone(),
            payload: Arc::new(payload.to_owned()),
        };
        self.send(subject, Received::Message(Arc::new(message)));
    }

    fn closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }
}

/// The adapter half the fixture's [`Handle`] wraps.
#[derive(Debug)]
struct Memory(Arc<Hub>);

#[async_trait]
impl Bus for Memory {
    async fn publish(&self, subject: &Subject, payload: &str) -> Result<()> {
        if self.0.closed() {
            return Err(ErrorKind::Closed.into());
        }
        self.0.deliver(subject, payload);
        Ok(())
    }

    async fn publish_batch(&self, subject: &Subject, payloads: &[String]) -> Result<()> {
        for payload in payloads {
            self.publish(subject, payload).await?;
        }
        Ok(())
    }

    async fn subscribe(&self, subject: &Subject) -> Result<Subscription> {
        if self.0.closed() {
            return Err(ErrorKind::Closed.into());
        }
        let receiver = self.0.sender(subject).subscribe();
        Ok(Subscription::new(stream::unfold(
            Some(receiver),
            async |state| {
                let mut receiver = state?;
                match receiver.recv().await {
                    Ok(delivered) => Some((Ok(delivered), Some(receiver))),
                    Err(broadcast::error::RecvError::Lagged(missed)) => {
                        Some((Ok(Received::Lagged(missed)), Some(receiver)))
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        Some((Err(ErrorKind::Closed.into()), None))
                    }
                }
            },
        )))
    }

    async fn shutdown(&self) -> Result<()> {
        self.0.closed.store(true, Ordering::Release);
        // Dropping every sender ends every reader's stream.
        self.0.channels().clear();
        Ok(())
    }

    async fn connected(&self) -> bool {
        !self.0.closed()
    }
}

/// A broker nobody can reach: every publish and subscribe fails.
#[derive(Debug)]
struct Unreachable;

#[async_trait]
impl Bus for Unreachable {
    async fn publish(&self, _subject: &Subject, _payload: &str) -> Result<()> {
        Err(ErrorKind::NatsDisconnected.into())
    }

    async fn publish_batch(&self, _subject: &Subject, _payloads: &[String]) -> Result<()> {
        Err(ErrorKind::NatsDisconnected.into())
    }

    async fn subscribe(&self, _subject: &Subject) -> Result<Subscription> {
        Err(ErrorKind::NatsDisconnected.into())
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn connected(&self) -> bool {
        false
    }
}

/// A bus whose every publish and subscribe fails as a disconnected broker's
/// does, for the suites that prove what a failure arm does with no bus.
#[must_use]
pub fn unreachable() -> Handle {
    Handle::new(Unreachable)
}

/// The server side of a detached bus: what a broker would send its readers.
#[derive(Debug, Clone)]
pub struct Server(Arc<Hub>);

/// A bus with in-process fan-out, and its server half; no broker or network
/// socket is required.
///
/// # Errors
/// Never in practice; the `Result` keeps every fixture call site fallible, as
/// a real adapter's connect is.
pub fn detached() -> Result<(Handle, Server)> {
    let hub = Arc::new(Hub::default());
    Ok((Handle::new(Memory(Arc::clone(&hub))), Server(hub)))
}

impl Server {
    /// Deliver `payload` to every reader of `subject`, as the broker would.
    pub fn publish(&self, subject: &Subject, payload: &str) {
        self.0.deliver(subject, payload);
    }

    /// Tell every reader of `subject` that frames may be missing, as a broker
    /// reconnect does.
    pub fn gap(&self, subject: &Subject) {
        self.0.send(subject, Received::Gap);
    }

    /// How many readers `subject` has right now; a dropped subscription stops
    /// counting at once.
    #[must_use]
    pub fn readers(&self, subject: &Subject) -> usize {
        self.0
            .channels()
            .get(&crate::nats::spelling(subject))
            .map_or(0, broadcast::Sender::receiver_count)
    }
}
