//! A hub with no connection, for the crates downstream of it to test against.
//!
//! What a reader sees — a frame, a lag, a gap, a closed hub — is decided by
//! the channel table, not by the socket. A downstream test that needs one of
//! those outcomes should get it from the same table the pushes feed, so this
//! stands in for the SERVER, not for the hub: [`Server::publish`] is an
//! `smessage` arriving and [`Server::confirm`] is an `ssubscribe`
//! confirmation, and the gap a repeated one earns is the real rule's.

use std::sync::Arc;

use tokio::sync::mpsc;

use super::channels::{Command, HubInner};
use super::{Message, SubscriptionHub};

/// What a detached hub's server would push down its connection.
#[derive(Debug)]
pub struct Server {
    inner: Arc<HubInner>,
    /// Held so a subscribe's command has somewhere to go; nothing reads it.
    _commands: mpsc::UnboundedReceiver<Command>,
}

impl SubscriptionHub {
    /// A hub that opens no connection, and the server a test drives it with.
    #[must_use]
    pub fn detached() -> (Self, Server) {
        let (commands, receiver) = mpsc::unbounded_channel();
        let inner = Arc::new(HubInner::new());
        let server = Server {
            inner: Arc::clone(&inner),
            _commands: receiver,
        };
        (Self { inner, commands }, server)
    }
}

impl Server {
    /// A frame published on `channel` arrives.
    pub fn publish(&self, channel: &str, payload: &str) {
        self.inner.dispatch(Message {
            channel: channel.to_owned(),
            payload: payload.to_owned(),
        });
    }

    /// The server confirms `channel`'s subscription. The second time is a
    /// subscription lost and restored, and its readers are sent a gap.
    pub fn confirm(&self, channel: &str) {
        self.inner.confirm(channel);
    }
}
