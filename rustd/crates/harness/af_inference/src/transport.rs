//! The reply bound agentsfleet sets on every model call: the runner frame,
//! since a reply larger than one frame cannot reach the daemon.

/// The bytes one model reply may carry, `af_wire::protocol::MESSAGE_BYTES_MAX`.
pub const REPLY_MAX_BYTES: usize = af_wire::protocol::MESSAGE_BYTES_MAX;
