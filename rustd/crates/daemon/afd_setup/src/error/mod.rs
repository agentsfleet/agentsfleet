//! One error hull; private kinds preserve their causes and safe public meaning.

use af_core::error_code::{self, ErrorCode};

pub(crate) mod raise;
#[cfg(test)]
mod tests;

af_core::error_shell!(
    /// A fleet create, read or change refused, or its store unreachable.
    pub struct Error(ErrorKind);
);

/// The result every fallible fleet operation returns.
pub type Result<T, E = Error> = std::result::Result<T, E>;

mod kind;
pub(crate) use kind::ErrorKind;

af_core::error_lifts!(Error, ErrorKind:
    afd_db::Error => Datastore,
    afd_credential::Error => Credential,
    af_fleet_runtime::Error => Name,
    af_crypto::error::Error => Entropy,
    af_core::error::Error => Identifier,
    garde::Report => Validation,
    afd_turns::Error => TurnWrite,
    afd_approval::Error => Approval,
    afd_fleet_lifecycle::Error => Lifecycle,
);

/// What a refusal of an archived fleet's edit tells the caller.
const DETAIL_ARCHIVED: &str =
    "This fleet is archived and cannot change; delete it or start a new fleet";
/// The state an archived fleet's refusal names: the fleet's stored status.
const CURRENT_ARCHIVED: &str = afd_fleet_lifecycle::FleetStatus::Archived.as_str();

impl Error {
    /// Stable registry classification; foreign failures retain their own codes.
    #[must_use]
    pub fn code(&self) -> ErrorCode {
        match self.kind() {
            ErrorKind::Hidden => error_code::AUTH_FORBIDDEN,
            ErrorKind::FleetMissing => error_code::AGENTSFLEET_NOT_FOUND,
            ErrorKind::Archived => error_code::AGENTSFLEET_ARCHIVED,
            ErrorKind::NameExists => error_code::AGENTSFLEET_NAME_EXISTS,
            ErrorKind::Stale { .. } => error_code::SETUP_STALE,
            ErrorKind::Precondition => error_code::SETUP_PRECONDITION,
            ErrorKind::Replay => error_code::SETUP_REPLAY,
            ErrorKind::Gone => error_code::SETUP_GONE,
            ErrorKind::Model => error_code::SETUP_MODEL,
            ErrorKind::Cursor => error_code::SETUP_CURSOR,
            ErrorKind::Validation { .. } | ErrorKind::Name { .. } | ErrorKind::Invalid { .. } => {
                error_code::INVALID_REQUEST
            }
            ErrorKind::Query { .. } | ErrorKind::Stored { .. } => error_code::INTERNAL_DB_QUERY,
            ErrorKind::Datastore { .. } => error_code::INTERNAL_DB_UNAVAILABLE,
            ErrorKind::Credential { source } => source.code(),
            ErrorKind::TurnWrite { source } => source.code(),
            ErrorKind::Approval { source } => source.code(),
            ErrorKind::Lifecycle { source } => source.code(),
            ErrorKind::Identifier { .. } | ErrorKind::Entropy { .. } => {
                error_code::INTERNAL_OPERATION_FAILED
            }
        }
    }

    /// A bounded sentence that never includes a request value or cause text.
    #[must_use]
    pub fn detail(&self) -> &'static str {
        match self.kind() {
            ErrorKind::Invalid { detail } => detail,
            ErrorKind::Archived => DETAIL_ARCHIVED,
            // The lifecycle's own sentence, so a purge refusal reads as the
            // schedule release's does.
            ErrorKind::Lifecycle { source } => source.detail(),
            ErrorKind::Validation { .. } | ErrorKind::Name { .. } => {
                "The request violates the supported shape or bounds"
            }
            ErrorKind::Hidden
            | ErrorKind::FleetMissing
            | ErrorKind::NameExists
            | ErrorKind::Stale { .. }
            | ErrorKind::Precondition
            | ErrorKind::Replay
            | ErrorKind::Gone
            | ErrorKind::Model
            | ErrorKind::TurnWrite { .. }
            | ErrorKind::Approval { .. }
            | ErrorKind::Cursor
            | ErrorKind::Query { .. }
            | ErrorKind::Stored { .. }
            | ErrorKind::Credential { .. }
            | ErrorKind::Datastore { .. }
            | ErrorKind::Identifier { .. }
            | ErrorKind::Entropy { .. } => af_core::problem::Problem::of(self.code()).title(),
        }
    }

    /// The current tag required on a precondition refusal.
    #[must_use]
    pub fn etag(&self) -> Option<&str> {
        match self.kind() {
            ErrorKind::Stale { etag } => Some(etag),
            _ => None,
        }
    }

    /// The bounded state required on a lifecycle conflict.
    #[must_use]
    pub const fn current_state(&self) -> Option<&'static str> {
        match self.kind() {
            ErrorKind::Archived => Some(CURRENT_ARCHIVED),
            ErrorKind::Replay => Some("accepted"),
            ErrorKind::NameExists => Some("name_exists"),
            ErrorKind::Model => Some("unavailable"),
            ErrorKind::Lifecycle { source } => source.current_state(),
            _ => None,
        }
    }

    /// Whether retrying after a datastore outage may succeed.
    #[must_use]
    pub fn is_datastore_unavailable(&self) -> bool {
        self.code() == error_code::INTERNAL_DB_UNAVAILABLE
    }
}
