//! The task that owns the pushes: frames out to readers, confirmations into
//! gaps, and a lost node's attribution.
//!
//! # Sharded pub/sub, and the push the cluster sends when a slot moves
//!
//! Every subscription is `SSUBSCRIBE`: the channel routes by its own slot, so
//! a publish reaches one node rather than being broadcast to all of them. The
//! server answers with RESP3 pushes on the same connection — `smessage` for a
//! frame, `ssubscribe` confirming a subscription, and `sunsubscribe` when the
//! node stops serving the channel. That last one is the case measured on the
//! local cluster: a slot migration strands the subscription, the old owner
//! pushes `sunsubscribe`, and the new owner counts zero subscribers until
//! someone subscribes again. So an `sunsubscribe` for a channel a reader still
//! holds is handed to the control task to re-issue, and one for a channel
//! nobody holds is the echo of our own `SUNSUBSCRIBE`.
//!
//! # Why this task never issues a command
//!
//! A command is a round trip, and a frame must never wait on one. Anything
//! that needs the socket is a [`Signal`] to the control task, which owns it.

use std::sync::Arc;

use redis::{PushInfo, PushKind, Value};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::Instant;

use super::Message;
use super::channels::{Confirmation, HubInner};
use super::gap::{Attribution, Burst, Loss, NODE_REPAIR_WINDOW};
use crate::topology::text;

/// What the dispatch task asks of the control task.
#[derive(Debug)]
pub(super) enum Signal {
    /// A slot moved under a channel a reader holds: subscribe it again.
    Resubscribe(String),
    /// A node's socket was lost: subscribe every one of these again, in place.
    /// Every channel, because the push does not say which node it was; see
    /// the `gap` module.
    Repair(Vec<String>),
    /// A loss no node repair explains: redial the whole connection.
    Redial(Loss),
}

/// Starts dispatching one connection's pushes. `redialled` names the channels
/// this connection re-subscribes because the previous one was lost.
pub(super) fn spawn(
    inner: Arc<HubInner>,
    pushes: mpsc::UnboundedReceiver<PushInfo>,
    signals: mpsc::UnboundedSender<Signal>,
    redialled: Vec<String>,
) -> JoinHandle<()> {
    let dispatch = Dispatch {
        inner,
        signals,
        attribution: Attribution::new(redialled),
        burst: Burst::default(),
    };
    tokio::spawn(run(dispatch, pushes))
}

/// One connection's dispatch state.
struct Dispatch {
    inner: Arc<HubInner>,
    signals: mpsc::UnboundedSender<Signal>,
    attribution: Attribution,
    burst: Burst,
}

/// Dispatches until the connection needs a redial or its pushes end.
///
/// Stops after asking for the redial: this connection is being replaced, and
/// every channel on its successor is gapped on confirmation anyway.
async fn run(mut dispatch: Dispatch, mut pushes: mpsc::UnboundedReceiver<PushInfo>) {
    let loss = loop {
        let wake = dispatch.wake();
        let redial = tokio::select! {
            push = pushes.recv() => match push {
                Some(push) => dispatch.push(push),
                None => Some(Loss::Closed),
            },
            () = sleep_until(wake) => dispatch.deadline(Instant::now()),
        };
        if let Some(loss) = redial {
            break loss;
        }
    };
    dispatch.burst.flush();
    // The control task is gone only when the hub is, and then nobody needs
    // the redial.
    let _ = dispatch.signals.send(Signal::Redial(loss));
}

/// Sleeps until `deadline`, or forever when there is none.
async fn sleep_until(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}

impl Dispatch {
    /// The next moment a timer needs this task: a pending loss's window, or
    /// the open gap warning.
    fn wake(&self) -> Option<Instant> {
        match (self.attribution.deadline(), self.burst.due()) {
            (Some(repair), Some(burst)) => Some(repair.min(burst)),
            (repair, burst) => repair.or(burst),
        }
    }

    /// Handles one push. `Some` means the connection needs a redial.
    fn push(&mut self, push: PushInfo) -> Option<Loss> {
        match push.kind {
            PushKind::SMessage => {
                if let Some(message) = message_of(push.data) {
                    self.inner.dispatch(message);
                }
            }
            PushKind::SSubscribe => self.confirmed(&push.data),
            PushKind::SUnsubscribe => self.unsubscribed(&push.data),
            // A node's socket died. The driver is already repairing the
            // socket; the hub re-subscribes what it holds on top of it.
            PushKind::Disconnection => {
                let live = self.inner.live_channels();
                let loss = self.attribution.disconnected(Instant::now(), &live);
                if loss.is_none() {
                    let channels = live.len();
                    let window_ms = NODE_REPAIR_WINDOW.as_millis();
                    tracing::info!(channels, window_ms, event = "hub_node_repairing");
                    let _ = self.signals.send(Signal::Repair(live));
                }
                return loss;
            }
            _other => {}
        }
        None
    }

    /// A subscription was confirmed; a repeat is a gap for its readers.
    fn confirmed(&mut self, data: &[Value]) {
        let Some(channel) = channel_of(data) else {
            return;
        };
        if self.inner.confirm(&channel) == Confirmation::Replay {
            let cause = self.attribution.replayed(&channel);
            self.burst.record(cause, Instant::now());
        }
    }

    /// The node stopped serving a channel — a slot moved. A reader still
    /// holding it is re-subscribed, which the new owner needs; a channel
    /// nobody holds is the echo of our own `SUNSUBSCRIBE`.
    fn unsubscribed(&mut self, data: &[Value]) {
        let Some(channel) = channel_of(data) else {
            return;
        };
        if !self.inner.holds_channel(&channel) {
            return;
        }
        // Hoisted: see the `tracing` note in the workspace Cargo.toml.
        let channel_name = channel.as_str();
        tracing::info!(channel = channel_name, event = "hub_subscription_moved");
        self.attribution.moved(channel.clone());
        let _ = self.signals.send(Signal::Resubscribe(channel));
    }

    /// A timer fired. `Some` means the pending loss went unexplained.
    fn deadline(&mut self, now: Instant) -> Option<Loss> {
        if self.burst.due().is_some_and(|due| now >= due) {
            self.burst.flush();
        }
        self.attribution.expired(now)
    }
}

/// An `smessage` push carries `[channel, payload]`.
fn message_of(data: Vec<Value>) -> Option<Message> {
    let mut fields = data.into_iter();
    let channel = text(&fields.next()?)?;
    let payload = text(&fields.next()?).unwrap_or_default();
    Some(Message { channel, payload })
}

/// An `ssubscribe` or `sunsubscribe` push carries `[channel, remaining]`.
fn channel_of(data: &[Value]) -> Option<String> {
    text(data.first()?)
}
