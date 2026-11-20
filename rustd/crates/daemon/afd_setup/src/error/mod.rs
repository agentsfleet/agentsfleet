//! One error hull; private kinds preserve their causes and safe public meaning.

use af_core::error_code::{self, ErrorCode};

pub(crate) mod raise;
#[cfg(test)]
mod tests;

af_core::error_shell!(
    /// A private-setup refusal or a failure to reach its durable store.
    pub struct Error(ErrorKind);
);

/// The result every fallible setup operation returns.
pub type Result<T, E = Error> = std::result::Result<T, E>;

mod kind;
pub(crate) use kind::ErrorKind;

af_core::error_lifts!(Error, ErrorKind:
    afd_cron::Error => Cron,
    af_context::Error => Context,
    afd_db::Error => Datastore,
    afd_credential::Error => Credential,
    af_fleet_runtime::Error => Definition,
    af_crypto::error::Error => Entropy,
    af_core::error::Error => Identifier,
    garde::Report => Validation,
    afd_runner::Error => Runner,
    afd_turns::Error => TurnWrite,
    afd_approval::Error => Approval,
    afd_fleet_lifecycle::Error => Lifecycle,
    afd_billing::Error => Billing,
);

/// What a refusal of an archived fleet's edit tells the caller.
const DETAIL_ARCHIVED: &str =
    "This fleet is archived and cannot change; delete it or set up a new fleet";
/// The state an archived fleet's refusal names: the fleet's stored status.
const CURRENT_ARCHIVED: &str = afd_fleet_lifecycle::FleetStatus::Archived.as_str();

impl Error {
    /// Stable registry classification; foreign failures retain their own codes.
    #[must_use]
    pub fn code(&self) -> ErrorCode {
        match self.kind() {
            ErrorKind::Cron { source } => source.code(),
            ErrorKind::Hidden => error_code::SETUP_NOT_FOUND,
            ErrorKind::FleetMissing => error_code::AGENTSFLEET_NOT_FOUND,
            ErrorKind::Archived => error_code::AGENTSFLEET_ARCHIVED,
            ErrorKind::NameExists => error_code::AGENTSFLEET_NAME_EXISTS,
            ErrorKind::Policy => error_code::SETUP_POLICY,
            ErrorKind::State { .. } => error_code::SETUP_STATE,
            ErrorKind::Stale { .. } => error_code::SETUP_STALE,
            ErrorKind::Precondition => error_code::SETUP_PRECONDITION,
            ErrorKind::Replay => error_code::SETUP_REPLAY,
            ErrorKind::Review { .. } => error_code::SETUP_REVIEW,
            ErrorKind::Gone => error_code::SETUP_GONE,
            ErrorKind::Model => error_code::SETUP_MODEL,
            ErrorKind::Balance => error_code::SETUP_BALANCE,
            ErrorKind::Token => error_code::SETUP_TOKEN,
            ErrorKind::Context { source } if source.is_window() => error_code::SETUP_TOKEN,
            ErrorKind::Context { source } => source.code(),
            ErrorKind::Cursor => error_code::SETUP_CURSOR,
            ErrorKind::Validation { .. }
            | ErrorKind::Definition { .. }
            | ErrorKind::Invalid { .. } => error_code::INVALID_REQUEST,
            ErrorKind::Query { .. } | ErrorKind::Stored { .. } | ErrorKind::StoredNumber { .. } => {
                error_code::INTERNAL_DB_QUERY
            }
            ErrorKind::Datastore { .. } => error_code::INTERNAL_DB_UNAVAILABLE,
            ErrorKind::Credential { source } => source.code(),
            ErrorKind::Runner { source } => source.code(),
            ErrorKind::TurnWrite { source } => source.code(),
            ErrorKind::Approval { source } => source.code(),
            ErrorKind::Lifecycle { source } => source.code(),
            ErrorKind::Billing { source } => source.code(),
            ErrorKind::Identifier { .. }
            | ErrorKind::Entropy { .. }
            | ErrorKind::Preview { .. } => error_code::INTERNAL_OPERATION_FAILED,
        }
    }

    /// A bounded sentence that never includes a request value or cause text.
    #[must_use]
    pub fn detail(&self) -> &'static str {
        match self.kind() {
            ErrorKind::Cron { source } => source.detail(),
            ErrorKind::Invalid { detail } => detail,
            ErrorKind::Archived => DETAIL_ARCHIVED,
            ErrorKind::Validation { .. } | ErrorKind::Definition { .. } => {
                "Setup input violates the supported shape or bounds"
            }
            ErrorKind::Context { .. }
            | ErrorKind::Hidden
            | ErrorKind::FleetMissing
            | ErrorKind::NameExists
            | ErrorKind::Policy
            | ErrorKind::State { .. }
            | ErrorKind::Stale { .. }
            | ErrorKind::Precondition
            | ErrorKind::Replay
            | ErrorKind::Review { .. }
            | ErrorKind::Gone
            | ErrorKind::Model
            | ErrorKind::Balance
            | ErrorKind::Runner { .. }
            | ErrorKind::TurnWrite { .. }
            | ErrorKind::Approval { .. }
            | ErrorKind::Lifecycle { .. }
            | ErrorKind::Token
            | ErrorKind::Preview { .. }
            | ErrorKind::Cursor
            | ErrorKind::Query { .. }
            | ErrorKind::Stored { .. }
            | ErrorKind::StoredNumber { .. }
            | ErrorKind::Credential { .. }
            | ErrorKind::Datastore { .. }
            | ErrorKind::Identifier { .. }
            | ErrorKind::Entropy { .. }
            | ErrorKind::Billing { .. } => af_core::problem::Problem::of(self.code()).title(),
        }
    }

    /// The current tag required on a precondition refusal.
    #[must_use]
    pub fn etag(&self) -> Option<&str> {
        match self.kind() {
            ErrorKind::Cron { source } => source.etag(),
            ErrorKind::Stale { etag } => Some(etag),
            ErrorKind::Context { .. }
            | ErrorKind::Hidden
            | ErrorKind::FleetMissing
            | ErrorKind::Archived
            | ErrorKind::NameExists
            | ErrorKind::Policy
            | ErrorKind::State { .. }
            | ErrorKind::Precondition
            | ErrorKind::Replay
            | ErrorKind::Review { .. }
            | ErrorKind::Gone
            | ErrorKind::Model
            | ErrorKind::Balance
            | ErrorKind::Runner { .. }
            | ErrorKind::TurnWrite { .. }
            | ErrorKind::Approval { .. }
            | ErrorKind::Lifecycle { .. }
            | ErrorKind::Token
            | ErrorKind::Preview { .. }
            | ErrorKind::Cursor
            | ErrorKind::Invalid { .. }
            | ErrorKind::Query { .. }
            | ErrorKind::Stored { .. }
            | ErrorKind::StoredNumber { .. }
            | ErrorKind::Validation { .. }
            | ErrorKind::Definition { .. }
            | ErrorKind::Credential { .. }
            | ErrorKind::Datastore { .. }
            | ErrorKind::Identifier { .. }
            | ErrorKind::Entropy { .. }
            | ErrorKind::Billing { .. } => None,
        }
    }

    /// Fresh daemon-computed digest, carried only by a stale-review refusal.
    #[must_use]
    pub fn review_digest(&self) -> Option<&str> {
        match self.kind() {
            ErrorKind::Review { digest } => Some(digest),
            ErrorKind::Context { .. }
            | ErrorKind::Cron { .. }
            | ErrorKind::Hidden
            | ErrorKind::FleetMissing
            | ErrorKind::Archived
            | ErrorKind::NameExists
            | ErrorKind::Policy
            | ErrorKind::State { .. }
            | ErrorKind::Stale { .. }
            | ErrorKind::Precondition
            | ErrorKind::Replay
            | ErrorKind::Gone
            | ErrorKind::Model
            | ErrorKind::Balance
            | ErrorKind::Runner { .. }
            | ErrorKind::TurnWrite { .. }
            | ErrorKind::Approval { .. }
            | ErrorKind::Lifecycle { .. }
            | ErrorKind::Token
            | ErrorKind::Preview { .. }
            | ErrorKind::Cursor
            | ErrorKind::Invalid { .. }
            | ErrorKind::Query { .. }
            | ErrorKind::Stored { .. }
            | ErrorKind::StoredNumber { .. }
            | ErrorKind::Validation { .. }
            | ErrorKind::Definition { .. }
            | ErrorKind::Credential { .. }
            | ErrorKind::Datastore { .. }
            | ErrorKind::Identifier { .. }
            | ErrorKind::Entropy { .. }
            | ErrorKind::Billing { .. } => None,
        }
    }

    /// The bounded state required on a lifecycle conflict.
    #[must_use]
    pub const fn current_state(&self) -> Option<&'static str> {
        match self.kind() {
            ErrorKind::Cron { source } => source.current_state(),
            ErrorKind::State { current } => Some(current),
            ErrorKind::Archived => Some(CURRENT_ARCHIVED),
            ErrorKind::Replay => Some("accepted"),
            ErrorKind::Review { .. } | ErrorKind::Policy => Some("review_required"),
            ErrorKind::NameExists => Some("name_exists"),
            ErrorKind::Model => Some("unavailable"),
            ErrorKind::Context { .. }
            | ErrorKind::Hidden
            | ErrorKind::FleetMissing
            | ErrorKind::Stale { .. }
            | ErrorKind::Precondition
            | ErrorKind::Gone
            | ErrorKind::Balance
            | ErrorKind::Runner { .. }
            | ErrorKind::TurnWrite { .. }
            | ErrorKind::Approval { .. }
            | ErrorKind::Lifecycle { .. }
            | ErrorKind::Token
            | ErrorKind::Preview { .. }
            | ErrorKind::Cursor
            | ErrorKind::Invalid { .. }
            | ErrorKind::Query { .. }
            | ErrorKind::Stored { .. }
            | ErrorKind::StoredNumber { .. }
            | ErrorKind::Validation { .. }
            | ErrorKind::Definition { .. }
            | ErrorKind::Credential { .. }
            | ErrorKind::Datastore { .. }
            | ErrorKind::Identifier { .. }
            | ErrorKind::Entropy { .. }
            | ErrorKind::Billing { .. } => None,
        }
    }

    /// Whether a server-side bound ended a statement (`afd_db::login`): the
    /// datastore said "not now", which a retry may get past.
    pub(crate) fn is_bound_reached(&self) -> bool {
        matches!(self.kind(), ErrorKind::Query { source, .. } if afd_db::error::is_bound_reached(source))
    }

    /// Whether retrying after a datastore outage may succeed.
    #[must_use]
    pub fn is_datastore_unavailable(&self) -> bool {
        self.code() == error_code::INTERNAL_DB_UNAVAILABLE
    }
}
