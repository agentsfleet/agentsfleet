//! The task that owns the pub/sub connection.
//!
//! Split from `hub.rs` per RULE FLL, along the seam that matters: `hub.rs` is
//! the refcount and what a reader sees, this is the connection and what
//! happens when it dies.
//!
//! # Sharded pub/sub, and the push the cluster sends when a slot moves
//!
//! Every subscription is `SSUBSCRIBE`: the channel routes by its own slot, so
//! a publish reaches one node rather than being broadcast to all of them. The
//! server answers with RESP3 pushes on the same connection — `smessage` for a
//! frame, and `sunsubscribe` when the node stops serving the channel. That
//! last one is the case measured on the local cluster: a slot migration
//! strands the subscription, the old owner pushes `sunsubscribe`, and the new
//! owner counts zero subscribers until someone subscribes again. So an
//! `sunsubscribe` for a channel a reader still holds is re-issued here, and
//! one for a channel nobody holds is the echo of our own `SUNSUBSCRIBE`.
//!
//! # The reconnect schedule is `backon`'s
//!
//! [`ExponentialBuilder`] carries the factor, floor, ceiling and a jitter the
//! library seeds itself, and the loop around it is `backon`'s `retry`: call a
//! fallible thing, sleep, call it again, until the cluster answers.

use std::sync::Arc;

use backon::{ExponentialBuilder, Retryable as _};
use redis::cluster_async::ClusterConnection;
use redis::{PushInfo, PushKind, Value};
use tokio::sync::mpsc;

use super::{Command, HubInner, Message};
use crate::config::RedisConfig;
use crate::error::{Error, Result};
use crate::topology::text;
use crate::transport;

/// Opens the first connection and leaves a task owning it.
///
/// The FIRST connection is awaited, so a hub that cannot reach the cluster at
/// boot fails boot rather than starting and reconnecting forever behind a
/// `/readyz` that says nothing is wrong.
pub(super) async fn spawn(
    config: RedisConfig,
    schedule: ExponentialBuilder,
    inner: Arc<HubInner>,
    commands: mpsc::UnboundedReceiver<Command>,
) -> Result<()> {
    let connection = connect(&config).await?;
    inner.record_connection();
    tokio::spawn(run(config, schedule, inner, commands, connection));
    Ok(())
}

/// A live pub/sub connection and the pushes the server sends down it.
struct Connection {
    connection: ClusterConnection,
    pushes: mpsc::UnboundedReceiver<PushInfo>,
}

async fn connect(config: &RedisConfig) -> Result<Connection> {
    let pushed = transport::connect_with_pushes(config, config.request_timeout()).await?;
    Ok(Connection {
        connection: pushed.connection,
        pushes: pushed.pushes,
    })
}

/// Pumps messages until the process ends, reconnecting whenever the
/// connection does.
async fn run(
    config: RedisConfig,
    schedule: ExponentialBuilder,
    inner: Arc<HubInner>,
    mut commands: mpsc::UnboundedReceiver<Command>,
    mut connection: Connection,
) {
    loop {
        // Anything subscribed before this connection existed — the whole map
        // after a reconnect — is subscribed again here. A reader that never
        // noticed the drop must not be left listening to nothing.
        resubscribe(&mut connection.connection, &inner.live_channels()).await;

        let dropped = pump(&inner, &mut commands, &mut connection).await;
        if !dropped {
            return; // the hub itself went away
        }

        // Hoisted: see the `tracing` note in the workspace Cargo.toml.
        let error_code = afd_core::error_code::STARTUP_REDIS_CONNECT.as_str();
        tracing::warn!(error_code, event = "hub_connection_dropped");

        connection = redial(&config, schedule).await;
        inner.record_connection();
        afd_observability::producers::http::hub_reconnected();
        tracing::info!(event = "hub_reconnected");
    }
}

/// Redials until the cluster answers, on the schedule the hub was started
/// with.
///
/// Infallible by signature, and that is the pub/sub contract: a reader holds a
/// receiver rather than a connection, so there is no caller to hand a failure
/// to and nothing sensible to do with one but try again. `production_backoff`
/// says so with `without_max_times` — the loop ends when the cluster comes
/// back and at no other point.
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

/// Serves one connection. Returns true when the connection died, false when
/// the hub was dropped and there is nothing left to serve.
async fn pump(
    inner: &Arc<HubInner>,
    commands: &mut mpsc::UnboundedReceiver<Command>,
    connection: &mut Connection,
) -> bool {
    loop {
        tokio::select! {
            command = commands.recv() => match command {
                Some(Command::Subscribe(channel)) => {
                    if connection.connection.ssubscribe(&channel).await.is_err() {
                        return true;
                    }
                }
                Some(Command::Unsubscribe(channel)) => {
                    if connection.connection.sunsubscribe(&channel).await.is_err() {
                        return true;
                    }
                }
                None => return false,
            },
            push = connection.pushes.recv() => match push {
                Some(PushInfo { kind: PushKind::SMessage, data }) => {
                    if let Some(message) = message_of(data) {
                        inner.dispatch(message);
                    }
                }
                // The node stopped serving the channel — a slot moved. A reader
                // still holding it is re-subscribed, which the new owner needs;
                // a channel nobody holds is the echo of our own SUNSUBSCRIBE.
                Some(PushInfo { kind: PushKind::SUnsubscribe, data }) => {
                    if let Some(channel) = channel_of(&data)
                        && inner.holds_channel(&channel)
                    {
                        // Hoisted: see the `tracing` note in the workspace Cargo.toml.
                        let channel_name = channel.as_str();
                        tracing::info!(channel = channel_name, event = "hub_subscription_moved");
                        if connection.connection.ssubscribe(&channel).await.is_err() {
                            return true;
                        }
                    }
                }
                // The driver reports a node's socket dying as a push; the
                // subscriptions on it are gone with it, and pub/sub has no
                // replay, so this is a fresh connection's job.
                Some(PushInfo { kind: PushKind::Disconnection, .. }) | None => return true,
                Some(_other_push) => {}
            },
        }
    }
}

/// Re-issues `SSUBSCRIBE` for every channel a reader still holds.
async fn resubscribe(connection: &mut ClusterConnection, channels: &[String]) {
    for channel in channels {
        if let Err(failure) = connection.ssubscribe(channel).await {
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

/// An `smessage` push carries `[channel, payload]`.
fn message_of(data: Vec<Value>) -> Option<Message> {
    let mut fields = data.into_iter();
    let channel = text(&fields.next()?)?;
    let payload = text(&fields.next()?).unwrap_or_default();
    Some(Message { channel, payload })
}

/// An `sunsubscribe` push carries `[channel, remaining]`.
fn channel_of(data: &[Value]) -> Option<String> {
    text(data.first()?)
}

