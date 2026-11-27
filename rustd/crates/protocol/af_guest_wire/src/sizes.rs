//! The sizes one guest agent link holds to, chosen by whoever starts it.
//!
//! The runner reads them from its file, starts the guest agent with them and
//! connects its client with the same: the guest agent clamps every read and caps
//! every line it reads by them, and the client caps the lines it reads and the
//! output it keeps unread. The constants are only what an unconfigured link
//! holds.

use crate::files::MAX_LISTING_BYTES;
use crate::output::EDGE_BYTES;
use crate::protocol::{MAX_FRAME_BYTES, MAX_READ_BYTES};

/// What one guest agent link holds to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sizes {
    /// The most of one file a single read answers with: a read asking for
    /// more is cut here and marked truncated.
    pub max_read_bytes: u64,
    /// The most of a process's unread output the client keeps at each edge,
    /// its head and its tail.
    pub edge_bytes: usize,
    /// The longest line either end reads. A read's answer, as base64, and a
    /// full listing must fit in it ([`Sizes::LISTING_BYTES`]).
    pub max_frame_bytes: usize,
}

impl Sizes {
    /// What every link held before the runner could choose.
    pub const DEFAULT: Self = Self {
        max_read_bytes: MAX_READ_BYTES,
        edge_bytes: EDGE_BYTES,
        max_frame_bytes: MAX_FRAME_BYTES,
    };

    /// The longest a full directory listing can serialize to: a frame no
    /// longer than this would end the connection on one.
    pub const LISTING_BYTES: usize = MAX_LISTING_BYTES;
}
