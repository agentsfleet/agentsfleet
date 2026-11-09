//! Bounded framing and native duplex transports shared by daemon and runner.
#![cfg_attr(not(test), deny(unused_crate_dependencies))]
#![deny(clippy::self_named_module_files)]

pub mod body;
pub mod client;
pub mod endpoint;
pub mod error;
pub mod framing;
mod h2;
mod h3;
mod jobs;
mod limits;
mod proxy;
pub mod server;
mod telemetry;
#[cfg(feature = "test-util")]
pub mod test_util;
pub mod tls;

pub use body::{Incoming, IncomingReader, Outgoing};
pub use client::{Connection, Transport};
pub use endpoint::Endpoint;
pub use error::{Error, Reason, Result, Stage};
pub use framing::{
    Budget, Encoder, MemoryLimits, MemorySettings, OwnedFrame, Reader, encode, write,
};
pub use h3::NetworkStats;
pub use server::{ConnectionInfo, Listener};

/// Upper bound on each native body transfer or pipe allocation.
pub const CHUNK_BYTES: usize = 16 * 1024;
pub(crate) const STREAM_WINDOW: u32 = 64 * 1024;
pub(crate) const CONNECTION_WINDOW: u32 = 512 * 1024;
pub(crate) const STREAMS: u32 = 4;
pub(crate) const HEADER_BYTES: u32 = 16 * 1024;
