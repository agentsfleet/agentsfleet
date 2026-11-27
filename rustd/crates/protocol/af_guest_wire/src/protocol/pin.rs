//! The wire of a save's pin: a walk of the workspace that reads only what
//! changed since the last committed pin, and the release that tells the guest
//! agent whether that pin's generation was committed.
//!
//! ```text
//!   fs/pin_walk { parent }  ─► { pin, transfer_id, delta, removed, … }
//!                               the changed entries read as an export
//!   fs/pin_release { pin, generation }   generation: committed, or none
//! ```

use serde::{Deserialize, Serialize};

/// `fs/pin_walk`: walk the workspace, keep each entry's identity for the
/// pin, and open an export of what changed since `parent`.
pub const METHOD_PIN_WALK: &str = "fs/pin_walk";
/// `fs/pin_release`: end a pin, keeping its identities as the committed
/// ones when its generation was committed.
pub const METHOD_PIN_RELEASE: &str = "fs/pin_release";

/// `fs/pin_walk` parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinWalkParams {
    /// The generation the last committed pin published, which the walk reads
    /// against; none reads the whole workspace.
    pub parent: Option<String>,
}

/// `fs/pin_walk` result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinWalked {
    /// The number the release names the pin by.
    pub pin: u64,
    /// The export of the changed entries, read as `fs/export_read` reads one.
    pub transfer_id: u64,
    /// Whether the export holds only what changed since `parent`; otherwise
    /// it holds the whole workspace.
    pub delta: bool,
    /// Each path `parent` held that is gone, none below another; empty unless
    /// `delta`.
    pub removed: Vec<String>,
    /// The entries the walk read the identity of.
    pub entries: u64,
    /// The bytes of the regular files the export carries.
    pub changed_bytes: u64,
}

/// `fs/pin_release` parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinReleaseParams {
    /// The pin.
    pub pin: u64,
    /// The generation the daemon committed from it; none when it committed
    /// nothing, so the last committed pin stays what the next walk reads
    /// against.
    pub generation: Option<String>,
}
