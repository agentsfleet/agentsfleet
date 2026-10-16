//! Conditional refusals expose only bounded state and the current version.

use super::{Error, ErrorKind};

/// The persisted sync state a held mutation names.
pub const STATE_SYNCING: &str = "syncing";
const STATE_DELETING: &str = "deleting";

impl Error {
    /// The fresh tag a stale schedule edit must display to the caller.
    #[must_use]
    pub fn etag(&self) -> Option<&str> {
        match self.kind() {
            ErrorKind::Stale { etag } => Some(etag),
            _ => None,
        }
    }

    /// Current state for a refusal whose registry status is conflict.
    #[must_use]
    pub fn current_state(&self) -> Option<&'static str> {
        match self.kind() {
            ErrorKind::Held | ErrorKind::RetireHeld => Some(STATE_SYNCING),
            ErrorKind::Deleting => Some(STATE_DELETING),
            ErrorKind::Refused { refusal } => refusal.current_state(),
            _ => None,
        }
    }
}
