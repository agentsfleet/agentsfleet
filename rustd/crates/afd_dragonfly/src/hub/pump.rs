//! The control task: the pub/sub connection's commands, and its redial.
//!
//! Split from `hub.rs` per RULE FLL, along the seam that matters: `hub.rs` is
//! the refcount and what a reader sees, this is the connection and what
//! happens when it dies. The pushes are [`dispatch`]'s, on a task of their
//! own, so a subscribe this task is waiting on never holds a frame.
//!
//! # One generation per connection
//!
//! ```text
//!   connect ─► spawn dispatch(pushes) ─► re-subscribe what readers hold
//!                    │                          │
//!                    │ Signal::Resubscribe      ▼
//!                    ├────────────────────► serve commands + signals
//!                    │ Signal::Redial               │
//!                    └────────────────────► abort dispatch, redial ─► next
//! ```
//!
//! A node's lost socket does NOT end a generation. The driver repairs the
//! node's socket inside the connection it already has — with no attempt cap,
//! see `transport::connect_with_pushes` — and this task re-subscribes every
//! channel on it at its owning primary (`repair`), each confirmation a gap.
//! The driver's own replay cannot be relied on to do it; `gap` says why.
//!
//! # The reconnect schedule is `backon`'s
//!
//! [`ExponentialBuilder`] carries the factor, floor, ceiling and a jitter the
//! library seeds itself, and the loop around it is `backon`'s `retry`: call a
//! fallible thing, sleep, call it again, until the cluster answers.

use std::sync::Arc;

use backon::{BackoffBuilder as _, ExponentialBuilder, Retryable as _};
use redis::PushInfo;
use redis::cluster_async::ClusterConnection;
use tokio::sync::mpsc;

use super::channels::{Command, HubInner};
use super::dispatch::{self, Signal};
use super::gap::Loss;
use super::repair;
use crate::config::DragonflyConfig;
use crate::error::{Error, Result};
use crate::transport;

/// Opens the first connection and leaves a task owning it.
///
/// The FIRST connection is awaited, so a hub that cannot reach the cluster at
/// boot fails boot rather than starting and reconnecting forever behind a
/// `/readyz` that says nothing is wrong.
pub(super) async fn spawn(
    config: DragonflyConfig,
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

async fn connect(config: &DragonflyConfig) -> Result<Connection> {
    let pushed = transport::connect_with_pushes(config, config.request_timeout()).await?;
    Ok(Connection {
        connection: pushed.connection,
        pushes: pushed.pushes,
    })
}

/// How one connection's generation ended.
enum Ended {
    /// Every handle on the hub dropped; there is nothing left to serve.
    HubDropped,
    /// The connection has to be redialled whole.
    Lost(Loss),
}

/// Serves connections until the process ends, redialling whenever one is
/// lost whole.
async fn run(
    config: DragonflyConfig,
    schedule: ExponentialBuilder,
    inner: Arc<HubInner>,
    mut commands: mpsc::UnboundedReceiver<Command>,
    first: Connection,
) {
    let Connection {
        mut connection,
        mut pushes,
    } = first;
    let mut redialled = false;
    loop {
        // After a redial, everything readers hold is subscribed again here: a
        // reader that never noticed the drop must not be left listening to
        // nothing, and the confirmation each re-subscribe earns is the gap it
        // is told about. The FIRST connection re-subscribes nothing, because
        // nothing was subscribed before it: a channel in the map already has
        // its own `Subscribe` queued, and subscribing it here as well would
        // confirm it twice — a gap for a reader that has lost nothing.
        let channels = if redialled {
            inner.live_channels()
        } else {
            Vec::new()
        };
        let (loss, unhosted) =
            match generation(&inner, &mut commands, &mut connection, pushes, channels).await {
                Ok(Ended::HubDropped) => return,
                Ok(Ended::Lost(loss)) => (loss, false),
                Err(Unhosted) => (Loss::CommandFailed, true),
            };

        // Hoisted: see the `tracing` note in the workspace Cargo.toml.
        let error_code = afd_core::error_code::STARTUP_DRAGONFLY_CONNECT.as_str();
        let cause = loss.as_str();
        tracing::warn!(cause, error_code, event = "hub_connection_dropped");
        // A cluster that accepted the dial but could not take the subscribes
        // would accept the next dial at once too: wait a step first.
        if unhosted {
            tokio::time::sleep(schedule.build().next().unwrap_or_default()).await;
        }

        Connection { connection, pushes } = redial(&config, schedule).await;
        redialled = true;
        inner.record_connection();
        afd_observability::producers::http::hub_reconnected();
        tracing::info!(event = "hub_reconnected");
    }
}

/// A redialled connection that could not take back every subscription.
struct Unhosted;

/// One connection's life: its pushes dispatched, what readers hold subscribed
/// again, then its commands served until it has to go.
///
/// A re-subscribe that fails ends the generation at once. Carrying on would
/// leave that channel's readers with neither frames nor a gap, which is the
/// silence the hub exists to prevent; a fresh connection re-subscribes it and
/// its confirmation is the gap.
async fn generation(
    inner: &Arc<HubInner>,
    commands: &mut mpsc::UnboundedReceiver<Command>,
    connection: &mut ClusterConnection,
    pushes: mpsc::UnboundedReceiver<PushInfo>,
    channels: Vec<String>,
) -> Result<Ended, Unhosted> {
    let (signal, mut signals) = mpsc::unbounded_channel();
    let dispatcher = dispatch::spawn(Arc::clone(inner), pushes, signal, channels.clone());
    let ended = match resubscribe(connection, &channels).await {
        Ok(()) => Ok(serve(commands, &mut signals, connection).await),
        Err(unhosted) => Err(unhosted),
    };
    dispatcher.abort();
    ended
}

/// Redials until the cluster answers, on the schedule the hub was started
/// with.
///
/// Infallible by signature, and that is the pub/sub rule: a reader holds a
/// receiver rather than a connection, so there is no caller to hand a failure
/// to and nothing sensible to do with one but try again. `production_backoff`
/// sets no attempt limit, but `start_with_backoff` takes the caller's schedule
/// as given, and a schedule that does set one ends its retry with an error. That
/// is not the end of the redial: the same schedule starts again, so the loop
/// ends when the cluster comes back and at no other point.
async fn redial(config: &DragonflyConfig, schedule: ExponentialBuilder) -> Connection {
    let mut attempt = 0_u32;
    loop {
        let redialled = (|| connect(config))
            .retry(schedule)
            .notify(|failure: &Error, _delay| {
                // Hoisted: see the `tracing` note in the workspace Cargo.toml.
                let error_code = afd_core::error_code::STARTUP_DRAGONFLY_CONNECT.as_str();
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
            .await;
        if let Ok(connection) = redialled {
            return connection;
        }
    }
}

/// Serves one connection's commands and the dispatch task's signals until the
/// connection has to go.
///
/// A command that fails is a lost connection: the driver has already retried
/// it through its own redirects and repairs, so what reaches here is a socket
/// that cannot be used.
async fn serve(
    commands: &mut mpsc::UnboundedReceiver<Command>,
    signals: &mut mpsc::UnboundedReceiver<Signal>,
    connection: &mut ClusterConnection,
) -> Ended {
    loop {
        let sent = tokio::select! {
            command = commands.recv() => match command {
                Some(Command::Subscribe(channel)) => connection.ssubscribe(&channel).await,
                Some(Command::Unsubscribe(channel)) => connection.sunsubscribe(&channel).await,
                None => return Ended::HubDropped,
            },
            // Dispatch ends only after signalling, so a closed signal channel
            // is a task that died without one: its pushes are unread either way.
            signal = signals.recv() => match signal.unwrap_or(Signal::Redial(Loss::Closed)) {
                Signal::Resubscribe(channel) => connection.ssubscribe(&channel).await,
                // The waiting a repair may do happens here, never on the
                // dispatch task: frames keep flowing while it waits.
                Signal::Repair(channels) => match repair::resubscribe(connection, &channels).await {
                    Ok(()) => Ok(()),
                    Err(loss) => return Ended::Lost(loss),
                },
                Signal::Redial(loss) => return Ended::Lost(loss),
            },
        };
        if sent.is_err() {
            return Ended::Lost(Loss::CommandFailed);
        }
    }
}

/// Re-issues `SSUBSCRIBE` for every channel a reader still holds, stopping at
/// the first the connection cannot take. The log names how many were left
/// unsubscribed, never which: a channel name is a fleet id.
async fn resubscribe(
    connection: &mut ClusterConnection,
    channels: &[String],
) -> Result<(), Unhosted> {
    for (index, channel) in channels.iter().enumerate() {
        if let Err(failure) = connection.ssubscribe(channel).await {
            // Hoisted: see the `tracing` note in the workspace Cargo.toml.
            let error_code = afd_core::error_code::STARTUP_DRAGONFLY_CONNECT.as_str();
            let channels = channels.len().saturating_sub(index);
            tracing::warn!(
                channels,
                error = %failure,
                error_code,
                event = "hub_resubscribe_failed"
            );
            return Err(Unhosted);
        }
    }
    Ok(())
}
