//! The public code and sentence are selected together.

use super::{Error, ErrorKind, detail};
use afd_core::error_code::{self, ErrorCode};

impl Error {
    /// The code and the sentence, decided together — see the module note.
    pub(super) fn answer(&self) -> (ErrorCode, &'static str) {
        match self.kind() {
            ErrorKind::Patch => (error_code::INVALID_REQUEST, detail::INVALID_PATCH),
            ErrorKind::Precondition => (error_code::AUTHORING_PRECONDITION, detail::TAG_REQUIRED),
            ErrorKind::Stale { .. } => (error_code::AUTHORING_STALE, detail::TAG_STALE),
            ErrorKind::Deleting => (error_code::AUTHORING_STATE, detail::DELETING),
            ErrorKind::Json { .. } => (error_code::INTERNAL_DB_QUERY, detail::DATABASE_ERROR),
            ErrorKind::Invalid { invalid } => (error_code::INVALID_REQUEST, invalid.detail()),
            ErrorKind::Refused { refusal } => (refusal.code(), refusal.detail()),
            ErrorKind::Held => (error_code::SCHEDULE_SYNCING, detail::SYNC_HELD),
            ErrorKind::Datastore { .. } => (
                error_code::INTERNAL_DB_UNAVAILABLE,
                detail::DATABASE_UNAVAILABLE,
            ),
            // The ledger already decided what a caller is told, and answering
            // a second sentence for one condition is the drift the shared
            // constants in `afd_core::error` exist to prevent.
            ErrorKind::Admission { source } => (source.code(), source.detail()),
            ErrorKind::Query { .. } | ErrorKind::RowUnreadable { .. } => {
                (error_code::INTERNAL_DB_QUERY, detail::DATABASE_ERROR)
            }
            // The vendor's own status is NOT surfaced to the caller. A person
            // editing a schedule cannot act on "QStash answered 429", and the
            // status is already in the row's `last_error` where an operator
            // reads it. What they are told is that the schedule is not yet
            // registered and that `:sync` will retry.
            ErrorKind::UpstreamUnreachable { .. } | ErrorKind::UpstreamRefused { .. } => (
                error_code::INTERNAL_OPERATION_FAILED,
                detail::UPSTREAM_UNAVAILABLE,
            ),
            ErrorKind::Exhausted | ErrorKind::Identifier { .. } => (
                error_code::INTERNAL_OPERATION_FAILED,
                detail::OPERATION_FAILED,
            ),
            ErrorKind::DestinationUnusable => (
                error_code::INTERNAL_OPERATION_FAILED,
                detail::DESTINATION_UNUSABLE,
            ),
            ErrorKind::DestinationNotPlainOrigin => (
                error_code::INTERNAL_OPERATION_FAILED,
                detail::DESTINATION_NOT_PLAIN_ORIGIN,
            ),
            ErrorKind::RetireHeld => (error_code::SCHEDULE_SYNCING, detail::RETIRE_HELD),
            // One answer for every refusal of a fire: telling a prober which
            // check it failed would say whether this daemon is misconfigured.
            ErrorKind::KeysMissing
            | ErrorKind::TokenMalformed
            | ErrorKind::SignatureInvalid
            | ErrorKind::BodyMismatch => (
                error_code::WEBHOOK_SIGNATURE_INVALID,
                detail::FIRE_UNVERIFIED,
            ),
        }
    }
}
