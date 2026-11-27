//! The host's client for the in-VM agent: processes and files inside one
//! sandbox, as the agent loop calls them.
//!
//! One agent, `agentsfleet-guest`, runs inside each run's microVM as the
//! guest's first process, and the supervisor reaches it over vsock, which the
//! host sees as a Unix socket (`docs/architecture/runner_execution.md`
//! §Process model). The messages both ends speak are `af_guest_wire`'s; this
//! crate links nothing of the agent's.
//!
//! # The seam
//!
//! [`GuestAgent`] is what the agent loop calls. It is a trait object because the
//! loop holds whichever engine's agent its run got, and because a test can
//! drive the loop with no sandbox at all. Each spawned [`Process`] carries its
//! own event channel, owned by the caller that spawned it, so no table of open
//! processes is shared behind a lock: the connection task owns the senders and
//! the caller owns the receivers.
//!
//! # The link
//!
//! [`Client`] implements [`GuestAgent`] over a socket, or over any byte stream
//! an [`AgentRoute`] dials and dials again when the link closes. It reads
//! the agent's messages as untrusted.

#![cfg_attr(not(test), deny(unused_crate_dependencies))]
#![deny(clippy::self_named_module_files)]

mod api;
mod client;
mod edges;
mod events;
mod slot;

pub use self::api::{GuestAgent, Process, ProcessEvent, ProcessId, Spawn};
pub use self::client::Client;
pub use self::client::route::{self, AgentRoute};
pub use self::events::{Events, Feed};
pub use self::slot::{AgentSlot, AgentSource, Outcome, Unavailable};
