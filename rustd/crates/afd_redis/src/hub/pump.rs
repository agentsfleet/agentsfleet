//! The task that owns the pub/sub socket.
//!
//! Split from `hub.rs` per RULE FLL, along the seam that matters: `hub.rs` is
//! the refcount and what a reader sees, this is the socket and what happens
//! when it dies.
//!
//! # Why the sink and the stream are split
//!
//! A `PubSub` connection cannot be read and commanded at the same time through
//! one handle — the message stream borrows it. `split()` gives a sink that
//! subscribes and a stream that yields, so a reader arriving mid-flight is
//! subscribed without interrupting delivery to everyone else.

use std::sync::Arc;

use futures_util::StreamExt as _;
use redis::aio::{PubSubSink, PubSubStream};

use super::{Backoff, Command, HubInner, Message};
use crate::config::RedisConfig;
use crate::error::{Error, ErrorKind};

/// Opens the first connection and leaves a task owning it.
///
/// The FIRST connection is awaited, so a hub that cannot reach Redis at boot
/// fails boot rather than starting and reconnecting forever behind a `/readyz`
/// that says nothing is wrong.
pub(super) async fn spawn(
    config: RedisConfig,
    backoff: Backoff,
    inner: Arc<HubInner>,
    commands: tokio::sync::mpsc::UnboundedReceiver<Command>,
) -> Result<(), Error> {
    let connection = connect(&config).await?;
    inner.record_connection();
    tokio::spawn(run(config, backoff, inner, commands, connection));
    Ok(())
}

/// A live pub/sub connection, split into its two halves.
struct Connection {
    sink: PubSubSink,
    stream: PubSubStream,
}

async fn connect(config: &RedisConfig) -> Result<Connection, Error> {
    let client = crate::client::build_client(config)?;
    let pubsub = client.get_async_pubsub().await.map_err(|source| {
        Error::new(ErrorKind::Unreachable {
            role: config.role().tag(),
            source: Box::new(source),
        })
    })?;

    let (sink, stream) = pubsub.split();
    Ok(Connection { sink, stream })
}

/// Pumps messages until the process ends, reconnecting whenever the socket does.
async fn run(
    config: RedisConfig,
    backoff: Backoff,
    inner: Arc<HubInner>,
    mut commands: tokio::sync::mpsc::UnboundedReceiver<Command>,
    mut connection: Connection,
) {
    let mut attempt = 0_u32;
    loop {
        // Anything subscribed before this connection existed — the whole map
        // after a reconnect — is subscribed again here. A reader that never
        // noticed the drop must not be left listening to nothing.
        resubscribe(&mut connection.sink, &inner.live_channels()).await;

        let dropped = pump(&inner, &mut commands, &mut connection).await;
        if !dropped {
            return; // the hub itself went away
        }

        // Hoisted: see the `tracing` note in the workspace Cargo.toml.
        let error_code = afd_core::error_code::STARTUP_REDIS_CONNECT.as_str();
        tracing::warn!(error_code, "hub_connection_dropped");

        connection = loop {
            let delay = backoff.delay(attempt, jitter());
            tokio::time::sleep(delay).await;
            attempt = attempt.saturating_add(1);
            match connect(&config).await {
                Ok(fresh) => break fresh,
                Err(failure) => {
                    let error_code = afd_core::error_code::STARTUP_REDIS_CONNECT.as_str();
                    tracing::warn!(
                        attempt,
                        error = %failure,
                        error_code,
                        "hub_reconnect_failed"
                    );
                }
            }
        };
        attempt = 0;
        inner.record_connection();
        tracing::info!("hub_reconnected");
    }
}

/// Serves one connection. Returns true when the socket died, false when the
/// hub was dropped and there is nothing left to serve.
async fn pump(
    inner: &Arc<HubInner>,
    commands: &mut tokio::sync::mpsc::UnboundedReceiver<Command>,
    connection: &mut Connection,
) -> bool {
    loop {
        tokio::select! {
            command = commands.recv() => match command {
                Some(Command::Subscribe(channel)) => {
                    if connection.sink.subscribe(&channel).await.is_err() {
                        return true;
                    }
                }
                Some(Command::Unsubscribe(channel)) => {
                    if connection.sink.unsubscribe(&channel).await.is_err() {
                        return true;
                    }
                }
                None => return false,
            },
            message = connection.stream.next() => match message {
                Some(message) => {
                    let channel = message.get_channel_name().to_owned();
                    let payload = message.get_payload::<String>().unwrap_or_default();
                    inner.dispatch(Message { channel, payload });
                }
                // The stream ending IS the connection dropping — pub/sub has no
                // other way to say it.
                None => return true,
            },
        }
    }
}

/// Re-issues `SUBSCRIBE` for every channel a reader still holds.
async fn resubscribe(sink: &mut PubSubSink, channels: &[String]) {
    for channel in channels {
        if let Err(failure) = sink.subscribe(channel).await {
            let error_code = afd_core::error_code::STARTUP_REDIS_CONNECT.as_str();
            tracing::warn!(
                channel,
                error = %failure,
                error_code,
                "hub_resubscribe_failed"
            );
        }
    }
}

/// Spread for the reconnect delay.
///
/// Derived from the process id and a MONOTONIC reading rather than a
/// random-number generator: the requirement is that two processes do not redial
/// in lockstep, not that the value be unpredictable, and this pulls in no
/// dependency.
///
/// Monotonic, not the wall clock, and the distinction is the whole point of the
/// spread. An operator correcting drift or an NTP step moves the wall clock
/// BACKWARD, which replays the same sub-second nanoseconds and hands two
/// redials the same jitter — reproducing the lockstep this exists to break, at
/// exactly the moment a cluster is most likely to be reconnecting at once.
/// [`std::time::Instant`] cannot be stepped, and cannot be confused with a
/// timestamp, which is why `afd_core::clock` deliberately exposes no monotonic
/// reading of its own.
fn jitter() -> u64 {
    /// Fixed at first use, so `elapsed` is a duration this process owns rather
    /// than an instant anybody could read as a date.
    static ORIGIN: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    let nanos = ORIGIN
        .get_or_init(std::time::Instant::now)
        .elapsed()
        .subsec_nanos();
    u64::from(nanos) ^ u64::from(std::process::id())
}
