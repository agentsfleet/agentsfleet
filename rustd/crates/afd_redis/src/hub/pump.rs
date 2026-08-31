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
//!
//! # The reconnect schedule is `backon`'s
//!
//! This file used to carry its own: a two-field `Backoff` that doubled, capped
//! and added a spread of at most a quarter, fed by a jitter source derived from
//! the process id and a monotonic reading. Every part of that is
//! [`ExponentialBuilder`] — factor, floor, ceiling, and a jitter the library
//! seeds itself — and the loop around it is `backon`'s `retry`, which is what
//! the redial has always been: call a fallible thing, sleep, call it again.
//!
//! The one behavioural change is the spread. The hand-rolled version added at
//! most 25% of the current delay; `backon` adds a random offset anywhere inside
//! it. Wider, which is the direction that breaks lockstep better.

use std::sync::Arc;

use backon::{ExponentialBuilder, Retryable as _};
use futures_util::StreamExt as _;
use redis::aio::{PubSubSink, PubSubStream};

use super::{Command, HubInner, Message};
use crate::config::RedisConfig;
use crate::error::{Error, ErrorKind, Result};

/// Opens the first connection and leaves a task owning it.
///
/// The FIRST connection is awaited, so a hub that cannot reach Redis at boot
/// fails boot rather than starting and reconnecting forever behind a `/readyz`
/// that says nothing is wrong.
pub(super) async fn spawn(
    config: RedisConfig,
    schedule: ExponentialBuilder,
    inner: Arc<HubInner>,
    commands: tokio::sync::mpsc::UnboundedReceiver<Command>,
) -> Result<()> {
    let connection = connect(&config).await?;
    inner.record_connection();
    tokio::spawn(run(config, schedule, inner, commands, connection));
    Ok(())
}

/// A live pub/sub connection, split into its two halves.
struct Connection {
    sink: PubSubSink,
    stream: PubSubStream,
}

async fn connect(config: &RedisConfig) -> Result<Connection> {
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
    schedule: ExponentialBuilder,
    inner: Arc<HubInner>,
    mut commands: tokio::sync::mpsc::UnboundedReceiver<Command>,
    mut connection: Connection,
) {
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
        tracing::warn!(error_code, event = "hub_connection_dropped");

        connection = redial(&config, schedule).await;
        inner.record_connection();
        tracing::info!(event = "hub_reconnected");
    }
}

/// Redials until Redis answers, on the schedule the hub was started with.
///
/// Infallible by signature, and that is the pub/sub contract: a reader holds a
/// receiver rather than a connection, so there is no caller to hand a failure
/// to and nothing sensible to do with one but try again. `production_backoff`
/// says so with `without_max_times` — the loop ends when Redis comes back and
/// at no other point.
///
/// `notify` is where the per-attempt line comes from, and it is `FnMut(&E,
/// Duration)` — `backon` counts attempts to drive its own schedule but does not
/// hand the number out, so the counter stays. What DID go with the old loop is
/// the reset: this counter is born at the start of one redial and dies when
/// Redis answers, where the previous one lived across reconnects and had to be
/// zeroed by hand afterwards.
async fn redial(config: &RedisConfig, schedule: ExponentialBuilder) -> Connection {
    let mut attempt = 0_u32;
    (|| connect(config))
        .retry(schedule)
        .notify(|failure: &Error, _delay| {
            // Hoisted: see the `tracing` note in the workspace Cargo.toml.
            let error_code = afd_core::error_code::STARTUP_REDIS_CONNECT.as_str();
            attempt = attempt.saturating_add(1);
            let count = attempt;
            let reason = failure.to_string();
            tracing::warn!(
                attempt = count,
                reason,
                error_code,
                event = "hub_reconnect_failed"
            );
        })
        .await
        // `without_max_times` has no terminal arm, so the only way out is a
        // connection. The arm exists because the signature still admits an
        // error, and it re-enters the same wait rather than inventing a
        // Connection that does not exist.
        .unwrap_or_else(|_unreachable| unreachable_redial())
}

/// The branch [`redial`]'s unlimited retry cannot reach.
fn unreachable_redial() -> ! {
    unreachable!("a redial with no attempt limit returns only on a connection")
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
                event = "hub_resubscribe_failed"
            );
        }
    }
}
