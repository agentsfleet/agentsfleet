//! The model providers: one [`Provider`] trait, the turn it streams, and the
//! wires behind it (`docs/architecture/runner_execution.md` §Crates).
//!
//! A [`Connect`] picks the route a lease's policy names and builds its
//! provider. rig speaks every wire; the runner owns which providers a lease
//! may reach, the transport under rig (one HTTP client, no redirect, bounded
//! retry) and the turn's chunks. The model key lives only here, in the
//! supervisor: no provider runs inside a sandbox.

#![deny(clippy::self_named_module_files)]

pub mod error;

mod connect;
mod image_input;
mod provider;
mod request;
mod retry;
mod transport;
mod turn;
mod usage;

pub use self::connect::{Connect, Connector};
pub use self::error::{Error, Result};
pub use self::image_input::{ImageInput, ImageKind};
pub use self::provider::{
    Call, Chunk, End, Hosted, Message, Provider, Replay, Request, ToolSpec, Usage,
};
pub use self::transport::REPLY_MAX_BYTES;
pub use afd_inference::log_filter;
pub use afd_wire::inference::InferenceApi as Wire;
