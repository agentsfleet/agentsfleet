//! A process's output as both ends name it: which stream a chunk came from,
//! and how much of each edge the host keeps unread when its starter names
//! none.

use bytes::Bytes;
use serde::{Deserialize, Serialize};

/// How much of each edge a process keeps unread when the client's starter
/// names none ([`crate::Sizes`]): 512 KiB of head and of tail, 1 MiB per
/// process.
pub const EDGE_BYTES: usize = 512 * 1024;

/// Which stream a chunk of output came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stream {
    /// The process's standard output, when it runs on pipes.
    Stdout,
    /// The process's standard error, when it runs on pipes.
    Stderr,
    /// The pseudo-terminal, which merges both.
    Terminal,
}

/// One chunk of output and the stream it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    /// Where it was written.
    pub stream: Stream,
    /// The bytes.
    pub data: Bytes,
}
