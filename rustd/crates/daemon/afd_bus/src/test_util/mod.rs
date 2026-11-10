//! An in-process Bus for the suites that need fan-out without a broker, and
//! the integration lane's live NATS for the suites that need one.
//!
//! Every subject is one broadcast channel: readers share each payload, a reader
//! that falls behind is told how many it lost, and the fixture's server half
//! injects messages and gaps the way a broker and its reconnects would.

use std::collections::HashMap;
use std::str::FromStr as _;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use async_trait::async_trait;
use futures_util::stream;
use tokio::sync::broadcast;

use af_core::env::{EnvSource as _, MapEnv, ProcessEnv};
use async_nats::ServerAddr;

use crate::nats::Tuning;
use crate::{Bus, Handle, Message, Received, Result, Subject, Subscription, error::ErrorKind};

mod proxy;

pub use proxy::Proxy;

/// Every subject's channel, keyed by its broker spelling, and whether the bus
/// has shut down.
#[derive(Debug, Default)]
struct Hub {
    channels: Mutex<HashMap<String, Channel>>,
    closed: AtomicBool,
}

/// One subject's readers.
#[derive(Debug)]
struct Channel {
    sender: broadcast::Sender<Received>,
}

impl Channel {
    fn new() -> Self {
        Self {
            // As far behind as the real adapter lets a reader fall before it
            // is told how many it lost.
            sender: broadcast::channel(Tuning::default().channel_capacity).0,
        }
    }
}

impl Hub {
    fn channels(&self) -> MutexGuard<'_, HashMap<String, Channel>> {
        // Each critical section leaves the map whole, so a panic elsewhere
        // cannot leave it half-written.
        self.channels.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// A new reader of `subject`.
    fn reader(&self, subject: &Subject) -> broadcast::Receiver<Received> {
        self.channels()
            .entry(crate::nats::spelling(subject))
            .or_insert_with(Channel::new)
            .sender
            .subscribe()
    }

    /// Hand `received` to every reader of `subject`. No reader is not an
    /// error: the bus is at most once.
    fn send(&self, subject: &Subject, received: Received) {
        let _ = self
            .channels()
            .entry(crate::nats::spelling(subject))
            .or_insert_with(Channel::new)
            .sender
            .send(received);
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
        // A shut bus refuses every operation, an empty batch included.
        if self.0.closed() {
            return Err(ErrorKind::Closed.into());
        }
        crate::publish_each(self, subject, payloads).await
    }

    async fn subscribe(&self, subject: &Subject) -> Result<Subscription> {
        if self.0.closed() {
            return Err(ErrorKind::Closed.into());
        }
        let receiver = self.0.reader(subject);
        Ok(Subscription::new(stream::unfold(
            Some(receiver),
            async |state| {
                let mut receiver = state?;
                match crate::subscription::received(receiver.recv().await) {
                    Ok(delivered) => Some((Ok(delivered), Some(receiver))),
                    Err(closed) => Some((Err(closed), None)),
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

/// The variable the integration lane exports with its NATS seeds.
pub const LANE_URL_ENV: &str = "TEST_NATS_URL";

/// A live connection to the integration lane's NATS cluster, on the deployed
/// subject spelling.
///
/// For the suites whose claim crosses the real broker. One handle per fixture:
/// clones share the connection, so a subscribe then a publish stay ordered.
///
/// # Errors
/// `UZ-STARTUP-001` when [`LANE_URL_ENV`] is unset, which means the suite ran
/// outside `make test-integration-rustd`; `UZ-STARTUP-004` when no seed
/// answers.
pub async fn lane() -> Result<Handle> {
    dial(&lane_url()?, None, Tuning::default()).await
}

/// [`lane`], with every subject under the development `namespace`, so a suite
/// running beside others never reads their traffic.
///
/// # Errors
/// As [`lane`], and `UZ-STARTUP-001` for a namespace outside its charset.
pub async fn lane_in(namespace: &str) -> Result<Handle> {
    dial(&lane_url()?, Some(namespace), Tuning::default()).await
}

/// Connect through the production boot path to `seeds`, comma-separated.
///
/// Subjects sit under `namespace` when one is named, and `tuning` replaces
/// the defaults. For a suite that dials one node, or a [`Proxy`] in front of
/// one.
///
/// # Errors
/// Whatever boot refuses: a malformed seed, namespace or limit as
/// `UZ-STARTUP-001`, and no seed answering as `UZ-STARTUP-004`.
pub async fn dial(seeds: &str, namespace: Option<&str>, tuning: Tuning) -> Result<Handle> {
    let mut pairs = vec![(crate::nats::URL_ENV, seeds)];
    pairs.extend(namespace.map(|namespace| (crate::nats::NAMESPACE_ENV, namespace)));
    let config = crate::nats::Config::from_env(&MapEnv::from_pairs(pairs))?.with_tuning(tuning)?;
    crate::nats::connect(&config).await
}

/// The integration lane's NATS seeds, one URL per node, in the order `make`
/// exports them.
///
/// # Errors
/// `UZ-STARTUP-001` when [`LANE_URL_ENV`] is unset, which means the suite ran
/// outside `make test-integration-rustd`.
pub fn lane_seeds() -> Result<Vec<String>> {
    Ok(lane_url()?.split(',').map(str::to_owned).collect())
}

/// The integration lane's NATS seeds as exported, comma-separated.
///
/// # Errors
/// `UZ-STARTUP-001` when [`LANE_URL_ENV`] is unset.
pub fn lane_url() -> Result<String> {
    Ok(ProcessEnv
        .get(LANE_URL_ENV)
        .ok_or(ErrorKind::NatsUrlUnset)?)
}

/// The port one seed dials.
///
/// # Errors
/// `UZ-STARTUP-001` when `seed` is not a NATS URL.
pub fn port_of(seed: &str) -> Result<u16> {
    Ok(seed_addr(seed)?.port())
}

/// `seed` with its port swapped for `port`, keeping its host and credentials,
/// such as to dial a [`Proxy`] in front of it.
///
/// # Errors
/// `UZ-STARTUP-001` when `seed` is not a NATS URL.
pub fn via(seed: &str, port: u16) -> Result<String> {
    let addr = seed_addr(seed)?;
    let credentials = match (addr.username(), addr.password()) {
        (Some(user), Some(password)) => format!("{user}:{password}@"),
        _ => String::new(),
    };
    Ok(format!("nats://{credentials}{}:{port}", addr.host()))
}

fn seed_addr(seed: &str) -> Result<ServerAddr> {
    Ok(ServerAddr::from_str(seed.trim()).map_err(|source| ErrorKind::NatsUrl { source })?)
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
    /// reconnect does. Each call is one gap.
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
            .map_or(0, |channel| channel.sender.receiver_count())
    }
}
