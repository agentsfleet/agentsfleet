//! Native process cleanup and executor reply deadlines shared by both ends.

use std::time::Duration;

/// How long a process group has between TERM and KILL.
pub const KILL_GRACE: Duration = Duration::from_secs(2);
/// How long output pipes may stay open after their leader ends.
/// Waiting on the writer is separate from waiting on an open process pipe;
/// the caller's ending deadline also bounds that reply delivery.
pub const DRAIN_GRACE: Duration = Duration::from_secs(2);
/// How long a native RPC waits for the executor's reply.
pub const CALL_DEADLINE: Duration = Duration::from_secs(30);
