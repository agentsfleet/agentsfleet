//! Core NATS behind the Bus: one connection per process, local fan-out, and a
//! gap wherever frames may have been lost.
//!
//! ```text
//! publish ──► client ──► 3-node core NATS ──► hub pump ──► every local reader
//!   fails ──► debt ──┐                 reconnect ─► Gap on every subscription
//! disconnect ─► recent subjects ─► debt ─► Af-Gap marker on each, once back
//! ```
//!
//! Core NATS holds no state and acknowledges nothing, so every signal here is
//! at most once. A gap tells the reader to backfill from Postgres.
//!
//! # A node that dies silently
//!
//! A host crash or a network split resets no connection. The client notices
//! only when its pings go unanswered (every 60 s, stale after two), so frames
//! published in the minutes before the noticed disconnect, older than the
//! debt window, reach no reader and carry no gap. Their rows are in Postgres;
//! a viewer sees them at its next `catching_up`. Accepted as a limit, with no
//! shorter ping configured.

mod config;
mod debt;
mod hub;
mod preflight;
mod signals;
mod tuning;

use std::sync::Arc;
use std::time::{Duration, Instant};

use af_core::error_code::{Coded as _, Logged};
use async_nats::{Client, ConnectOptions};
use async_trait::async_trait;
use bytes::Bytes;
use tokio::{sync::RwLock, task::JoinHandle};

use crate::{Bus, Handle, Result, Subject, Subscription, error::ErrorKind};

pub(crate) use config::MAX_NAMESPACE_BYTES;
pub use config::{Config, NAMESPACE_ENV, URL_ENV};
pub use tuning::Tuning;

use self::{hub::Channels, signals::Signals};

/// The subject every hub reads; one marker on it gaps every subscription.
const GAP_ALL: &str = "af.bus.gap";

/// Connect to the seeds, refuse an unsuitable server, and start the hub.
///
/// # Errors
/// `UZ-STARTUP-004` when no seed answers within the connect deadline or the
/// server fails the boot check.
pub async fn connect(config: &Config) -> Result<Handle> {
    let seeds = config.seeds.len();
    let timeout_ms = config.tuning().connect_timeout().as_millis();
    tracing::info!(seeds, timeout_ms, event = "bus_connect_started");
    let started = Instant::now();
    let outcome = connect_inner(config).await;
    let duration_ms = started.elapsed().as_millis();
    match &outcome {
        Ok(_bus) => tracing::info!(seeds, duration_ms, event = "bus_connect_completed"),
        Err(failure) => {
            let Logged { error_code, reason } = failure.logged();
            tracing::warn!(
                seeds,
                duration_ms,
                error_code,
                reason,
                event = "bus_connect_failed"
            );
        }
    }
    outcome
}

/// The connect itself, so [`connect`] logs one outcome on every exit path.
async fn connect_inner(config: &Config) -> Result<Handle> {
    let tuning = config.tuning();
    let signals = Arc::new(Signals::new(tuning));
    let observer = Arc::clone(&signals);
    let client = options_for(config.credentials.as_ref())
        .connection_timeout(tuning.connect_timeout())
        .subscription_capacity(tuning.subscription_capacity)
        .event_callback(move |event| {
            let observer = Arc::clone(&observer);
            async move { observer.observe(&event, Instant::now()) }
        })
        .connect(config.seeds.clone())
        .await?;
    preflight::check(&client.server_info())?;
    let adapter = Adapter::start(client, config.prefix.clone(), tuning, signals).await?;
    Ok(Handle::new(adapter))
}

/// The client authenticates only from options, never from a URL's userinfo.
fn options_for(credentials: Option<&(String, String)>) -> ConnectOptions {
    credentials.map_or_else(ConnectOptions::new, |(user, pass)| {
        ConnectOptions::with_user_and_password(user.clone(), pass.clone())
    })
}

#[derive(Debug)]
struct Adapter {
    prefix: String,
    deadline: Duration,
    // Shared reads let operations run concurrently; shutdown takes the write
    // lock, so nothing opens after the drain begins.
    live: RwLock<Option<Live>>,
}

#[derive(Debug)]
struct Live {
    client: Client,
    channels: Arc<Channels>,
    signals: Arc<Signals>,
    watcher: JoinHandle<()>,
}

impl Adapter {
    async fn start(
        client: Client,
        prefix: String,
        tuning: &Tuning,
        signals: Arc<Signals>,
    ) -> Result<Self> {
        let channels = Arc::new(Channels::new(tuning.channel_capacity));
        let everyone = client.subscribe(format!("{prefix}{GAP_ALL}")).await?;
        client.flush().await?;
        let watcher = tokio::spawn(signals::watch(
            client.clone(),
            Arc::clone(&channels),
            Arc::clone(&signals),
            everyone,
            format!("{prefix}{GAP_ALL}"),
        ));
        Ok(Self {
            prefix,
            deadline: tuning.publish_deadline(),
            live: RwLock::new(Some(Live {
                client,
                channels,
                signals,
                watcher,
            })),
        })
    }

    fn physical(&self, subject: &Subject) -> String {
        format!("{}{}", self.prefix, spelling(subject))
    }
}

/// Each subject's NATS spelling, before the development prefix.
#[must_use]
pub fn spelling(subject: &Subject) -> String {
    match subject {
        Subject::FleetTail(fleet) => format!("af.fleet.{fleet}.tail"),
        Subject::DraftEvents(draft) => format!("af.draft.{draft}.events"),
        Subject::Wake(shard) => format!("af.wake.{shard}"),
        Subject::RunnerCommand(runner) => format!("af.runner.{runner}.cmd"),
        Subject::OutboundWake => "af.outbound.wake".to_owned(),
    }
}

async fn send(client: &Client, physical: String, payload: Bytes, deadline: Duration) -> Result<()> {
    tokio::time::timeout(deadline, client.publish(physical, payload)).await??;
    Ok(())
}

#[async_trait]
impl Bus for Adapter {
    async fn publish(&self, subject: &Subject, payload: &str) -> Result<()> {
        let live = self.live.read().await;
        let live = live.as_ref().ok_or(ErrorKind::Closed)?;
        let physical = self.physical(subject);
        let outcome = if live.signals.blocked() {
            Err(ErrorKind::NatsDisconnected.into())
        } else {
            let payload = Bytes::copy_from_slice(payload.as_bytes());
            send(&live.client, physical.clone(), payload, self.deadline).await
        };
        live.signals
            .record(&physical, outcome.is_ok(), Instant::now());
        outcome
    }

    async fn publish_batch(&self, subject: &Subject, payloads: &[String]) -> Result<()> {
        // A shut adapter refuses every operation, an empty batch included.
        if self.live.read().await.is_none() {
            return Err(ErrorKind::Closed.into());
        }
        crate::publish_each(self, subject, payloads).await
    }

    async fn subscribe(&self, subject: &Subject) -> Result<Subscription> {
        let live = self.live.read().await;
        let live = live.as_ref().ok_or(ErrorKind::Closed)?;
        let physical = self.physical(subject);
        let hub::Opened {
            reader,
            pump,
            mut subscribed,
        } = live.channels.open(&physical);
        if let Some(pump) = pump {
            let native = match live.client.subscribe(physical.clone()).await {
                Ok(native) => native,
                Err(source) => {
                    live.channels.ended(&physical, pump.generation);
                    return Err(source.into());
                }
            };
            let task = tokio::spawn(hub::pump(
                native,
                pump.sender,
                subject.clone(),
                Arc::clone(&live.channels),
                physical.clone(),
                pump.generation,
            ));
            live.channels.started(&physical, pump.generation, task);
            pump.subscribed.send_replace(true);
        }
        // The client writes commands in order, and `flush` returns once they
        // leave the socket, not once the server answers: every reader returns
        // with the channel's subscription on the wire. A joiner waits for its
        // opener to queue it; an opener that fails or is cancelled drops the
        // signal, and its joiners fail. The interest reaches the cluster after
        // this returns, which is why a live stream backfills once subscribed.
        tokio::time::timeout(self.deadline, async {
            subscribed.wait_for(|queued| *queued).await?;
            live.client.flush().await?;
            Ok::<_, crate::Error>(())
        })
        .await??;
        Ok(reader.into_subscription())
    }

    async fn connected(&self) -> bool {
        self.live.read().await.as_ref().is_some_and(|live| {
            live.client.connection_state() == async_nats::connection::State::Connected
        })
    }

    async fn shutdown(&self) -> Result<()> {
        let Some(live) = self.live.write().await.take() else {
            return Ok(());
        };
        // Every task this adapter started is stopped and joined before the
        // client drains, so nothing it owns outlives the call, even while a
        // subscriber still holds its stream.
        live.watcher.abort();
        joined("bus_watcher_panicked", live.watcher.await);
        for pump in live.channels.close() {
            joined("bus_pump_panicked", pump.await);
        }
        live.client.drain().await?;
        Ok(())
    }
}

/// A stopped task's join. Cancellation is how shutdown stops it; a panic is a
/// defect it hid until now, so it is logged rather than dropped.
fn joined(event: &'static str, join: core::result::Result<(), tokio::task::JoinError>) {
    if let Err(failure) = join
        && failure.is_panic()
    {
        let error_code = af_core::error_code::INTERNAL_OPERATION_FAILED.as_str();
        tracing::warn!(
            event,
            error_code,
            "a bus task panicked before shutdown joined it"
        );
    }
}

#[cfg(test)]
mod tests;
