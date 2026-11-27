//! What the client's link runs over: any byte stream a [`AgentRoute`] dials.
//!
//! The JSON lines are the same over each. A sandbox that inherits a socket
//! pair is dialled once, with the stream it was handed; a VM's guest agent is
//! dialled again after every boot, so a link a park closed comes back.

use std::fmt;
use std::io;

use af_guest_wire::AgentStream;

/// How a client reaches its guest agent, once at the start and again whenever
/// a call finds the link closed.
#[async_trait::async_trait]
pub trait AgentRoute: Send + Sync + fmt::Debug {
    /// A fresh stream to the guest agent.
    ///
    /// # Errors
    /// The guest agent cannot be reached, or this route dials no more.
    async fn dial(&self) -> io::Result<Box<dyn AgentStream>>;
}
