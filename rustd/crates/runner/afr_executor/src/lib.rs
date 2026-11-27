//! The executor: processes and files inside one sandbox.
//!
//! One executor runs inside each run's microVM as the guest's root service,
//! and the supervisor reaches it over vsock, which the host sees as a Unix
//! socket (`docs/architecture/runner_execution.md` §Process model).
//!
//! # The seam
//!
//! [`Executor`] is what the agent loop calls. It is a trait object because the
//! loop holds whichever engine's executor its run got, and because a test can
//! drive the loop with no sandbox at all. Each spawned [`Process`] carries its
//! own event channel, owned by the caller that spawned it, so no table of open
//! processes is shared behind a lock: the connection task owns the senders and
//! the caller owns the receivers.
//!
//! # The two ends
//!
//! [`bind`] claims a socket path, and [`streamed`] takes a connected byte
//! stream, a virtual machine's vsock connection; [`Server::serve`] serves
//! that one connection, owning every process it starts. Taking the link is
//! split from serving so a caller takes it before it confines itself.
//! [`Client`] is the supervisor's end and implements [`Executor`] over the
//! socket, or over any byte stream an [`ExecutorRoute`] dials and dials again
//! when the link closes. Both read the other's messages as untrusted.

#![cfg_attr(not(test), deny(unused_crate_dependencies))]
#![deny(clippy::self_named_module_files)]

pub mod error;
pub mod timing;
pub mod tree;

mod api;
mod client;
mod edges;
mod events;
mod protocol;
mod server;
mod sizes;
mod slot;

pub use self::api::{
    DirEntry, Ending, EntryKind, Executor, FileContent, Found, Hit, Imported, Listing, Pattern,
    Process, ProcessEvent, ProcessId, Search, Spawn, Stream,
};
pub use self::client::Client;
pub use self::client::route::{self, ExecutorRoute};
pub use self::edges::EDGE_BYTES;
pub use self::error::{Error, Result};
pub use self::events::{Events, Feed};
pub use self::protocol::{MAX_READ_BYTES, READ_CHUNK_BYTES, TRANSFER_CHUNK_BYTES};
pub use self::server::{
    Entry, InnerEnvironment, Server, Tenant, WORKSPACE_ROOT, bind, serve, streamed,
};
pub use self::sizes::Sizes;
pub use self::slot::{ExecutorSlot, ExecutorSource, Outcome, Unavailable};
