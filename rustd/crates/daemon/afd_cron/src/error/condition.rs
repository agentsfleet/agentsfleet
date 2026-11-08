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
            ErrorKind::Patch
            | ErrorKind::Precondition
            | ErrorKind::Deleting
            | ErrorKind::Json { .. }
            | ErrorKind::Invalid { .. }
            | ErrorKind::Refused { .. }
            | ErrorKind::Held
            | ErrorKind::Exhausted
            | ErrorKind::Datastore { .. }
            | ErrorKind::Query { .. }
            | ErrorKind::Identifier { .. }
            | ErrorKind::Admission { .. }
            | ErrorKind::UpstreamUnreachable { .. }
            | ErrorKind::UpstreamRefused { .. }
            | ErrorKind::RowUnreadable { .. }
            | ErrorKind::RetireHeld
            | ErrorKind::KeysMissing
            | ErrorKind::TokenMalformed
            | ErrorKind::SignatureInvalid
            | ErrorKind::BodyMismatch
            | ErrorKind::DestinationUnusable
            | ErrorKind::DestinationNotPlainOrigin => None,
        }
    }

    /// Current state for a refusal whose registry status is conflict.
    #[must_use]
    pub const fn current_state(&self) -> Option<&'static str> {
        match self.kind() {
            ErrorKind::Held | ErrorKind::RetireHeld => Some(STATE_SYNCING),
            ErrorKind::Deleting => Some(STATE_DELETING),
            ErrorKind::Refused { refusal } => refusal.current_state(),
            ErrorKind::Patch
            | ErrorKind::Precondition
            | ErrorKind::Stale { .. }
            | ErrorKind::Json { .. }
            | ErrorKind::Invalid { .. }
            | ErrorKind::Exhausted
            | ErrorKind::Datastore { .. }
            | ErrorKind::Query { .. }
            | ErrorKind::Identifier { .. }
            | ErrorKind::Admission { .. }
            | ErrorKind::UpstreamUnreachable { .. }
            | ErrorKind::UpstreamRefused { .. }
            | ErrorKind::RowUnreadable { .. }
            | ErrorKind::KeysMissing
            | ErrorKind::TokenMalformed
            | ErrorKind::SignatureInvalid
            | ErrorKind::BodyMismatch
            | ErrorKind::DestinationUnusable
            | ErrorKind::DestinationNotPlainOrigin => None,
        }
    }
}
