//! What the host and the in-VM agent both speak: the messages on the link
//! between them, their framing, the values each end reads, the refusals
//! both name, and what the guest kernel's command line tells the agent.
//!
//! The runner (`afr_guest_client`) and `agentsfleet-guest` (`afg_agent`)
//! depend on this crate and on nothing of each other's, so the agent that
//! runs tenant code inside a microVM never links the host's code, and the
//! host never links the agent's. Both read the other's messages as
//! untrusted: every message decodes through the object-only gate.
//!
//! - [`protocol`]: JSON-RPC 2.0, one message per line, its methods, its
//!   parameters and results, and the codes a refusal is answered with.
//! - [`error`]: the one failure type both ends raise and compare on.
//! - [`tree`]: the tar stream a tree crosses the link as, packed and
//!   unpacked the same way on either end.
//! - [`GuestEntry`]: the agent's flags, written by the engine and parsed by
//!   the agent from one type.

#![cfg_attr(not(test), deny(unused_crate_dependencies))]
#![deny(clippy::self_named_module_files)]

pub mod error;
pub mod protocol;
pub mod timing;
pub mod tree;

mod ending;
mod entry;
mod files;
mod output;
mod search;
mod sizes;
mod stream;

pub use self::ending::{Ending, SHELL_KILLED_EXIT, SIGKILL};
pub use self::entry::{AGENT_PORT, GUEST_USER, GuestEntry, HOST_CID};
pub use self::error::{Error, Result};
pub use self::files::{
    DirEntry, EntryKind, FileContent, Imported, Listing, MAX_LIST_ENTRIES, MAX_LISTING_BYTES,
};
pub use self::output::{Chunk, EDGE_BYTES, Stream};
pub use self::protocol::{MAX_READ_BYTES, READ_CHUNK_BYTES, TRANSFER_CHUNK_BYTES};
pub use self::search::{Found, Hit, Pattern, Search};
pub use self::sizes::Sizes;
pub use self::stream::AgentStream;

/// Where the workspace disk is mounted inside every sandbox: the root the
/// agent confines every path to, and the one the host's tools name.
pub const WORKSPACE_ROOT: &str = "/workspace";
