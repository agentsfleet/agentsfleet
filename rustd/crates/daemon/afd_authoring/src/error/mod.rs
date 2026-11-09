//! One error hull; private kinds preserve their causes and safe public meaning.

use af_core::error_code::{self, ErrorCode};

pub(crate) mod raise;
#[cfg(test)]
mod tests;

af_core::error_shell!(
    /// A private-setup refusal or a failure to reach its durable store.
    pub struct Error(ErrorKind);
);

/// The result every fallible authoring operation returns.
pub type Result<T, E = Error> = std::result::Result<T, E>;

mod kind;
pub(crate) use kind::ErrorKind;

af_core::error_lifts!(Error, ErrorKind:
    af_providers::Error => Decision,
    afd_cron::Error => Cron,
    af_context::Error => Context,
    afd_db::Error => Datastore,
    afd_credential::Error => Credential,
    af_fleet_runtime::Error => Definition,
    af_crypto::error::Error => Entropy,
    af_core::error::Error => Identifier,
    garde::Report => Validation,
    reqwest::Error => Transport,
    af_inference::Error => Inference,
    rig_core::ProviderError => Sdk,
    afd_runner::Error => Runner,
    afd_admission::Error => Admission,
    afd_approval::Error => Approval,
    afd_billing::Error => Billing,
);

impl Error {
    /// Stable registry classification; foreign failures retain their own codes.
    #[must_use]
    pub fn code(&self) -> ErrorCode {
        match self.kind() {
            ErrorKind::Cron { source } => source.code(),
            ErrorKind::Hidden => error_code::AUTHORING_NOT_FOUND,
            ErrorKind::FleetMissing => error_code::AGENTSFLEET_NOT_FOUND,
            ErrorKind::NameExists => error_code::AGENTSFLEET_NAME_EXISTS,
            ErrorKind::Policy => error_code::AUTHORING_POLICY,
            ErrorKind::Fence => error_code::AUTHORING_FENCE,
            ErrorKind::State { .. } => error_code::AUTHORING_STATE,
            ErrorKind::Stale { .. } => error_code::AUTHORING_STALE,
            ErrorKind::Precondition => error_code::AUTHORING_PRECONDITION,
            ErrorKind::Replay => error_code::AUTHORING_REPLAY,
            ErrorKind::Review { .. } => error_code::AUTHORING_REVIEW,
            ErrorKind::Gone => error_code::AUTHORING_GONE,
            ErrorKind::Model | ErrorKind::Inference { .. } => error_code::AUTHORING_MODEL,
            ErrorKind::Balance => error_code::AUTHORING_BALANCE,
            ErrorKind::Token => error_code::AUTHORING_TOKEN,
            ErrorKind::Context { source } if source.is_window() => error_code::AUTHORING_TOKEN,
            ErrorKind::Context { source } => source.code(),
            ErrorKind::Provider => error_code::AUTHORING_PROVIDER,
            ErrorKind::Unknown
            | ErrorKind::Transport { .. }
            | ErrorKind::Sdk { .. }
            | ErrorKind::Decision { .. } => error_code::AUTHORING_UNKNOWN,
            ErrorKind::Cursor => error_code::AUTHORING_CURSOR,
            ErrorKind::Validation { .. }
            | ErrorKind::Definition { .. }
            | ErrorKind::Invalid { .. } => error_code::INVALID_REQUEST,
            ErrorKind::Query { .. } | ErrorKind::Stored { .. } => error_code::INTERNAL_DB_QUERY,
            ErrorKind::Datastore { .. } => error_code::INTERNAL_DB_UNAVAILABLE,
            ErrorKind::Credential { source } => source.code(),
            ErrorKind::Runner { source } => source.code(),
            ErrorKind::Admission { source } => source.code(),
            ErrorKind::Approval { source } => source.code(),
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
            ErrorKind::Validation { .. } | ErrorKind::Definition { .. } => {
                "Setup input violates the supported shape or bounds"
            }
            ErrorKind::Context { .. }
            | ErrorKind::Hidden
            | ErrorKind::FleetMissing
            | ErrorKind::NameExists
            | ErrorKind::Policy
            | ErrorKind::Fence
            | ErrorKind::State { .. }
            | ErrorKind::Stale { .. }
            | ErrorKind::Precondition
            | ErrorKind::Replay
            | ErrorKind::Review { .. }
            | ErrorKind::Gone
            | ErrorKind::Model
            | ErrorKind::Balance
            | ErrorKind::Runner { .. }
            | ErrorKind::Admission { .. }
            | ErrorKind::Approval { .. }
            | ErrorKind::Token
            | ErrorKind::Provider
            | ErrorKind::Unknown
            | ErrorKind::Decision { .. }
            | ErrorKind::Inference { .. }
            | ErrorKind::Sdk { .. }
            | ErrorKind::Transport { .. }
            | ErrorKind::Preview { .. }
            | ErrorKind::Cursor
            | ErrorKind::Query { .. }
            | ErrorKind::Stored { .. }
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
            | ErrorKind::NameExists
            | ErrorKind::Policy
            | ErrorKind::Fence
            | ErrorKind::State { .. }
            | ErrorKind::Precondition
            | ErrorKind::Replay
            | ErrorKind::Review { .. }
            | ErrorKind::Gone
            | ErrorKind::Model
            | ErrorKind::Balance
            | ErrorKind::Runner { .. }
            | ErrorKind::Admission { .. }
            | ErrorKind::Approval { .. }
            | ErrorKind::Token
            | ErrorKind::Provider
            | ErrorKind::Unknown
            | ErrorKind::Decision { .. }
            | ErrorKind::Inference { .. }
            | ErrorKind::Sdk { .. }
            | ErrorKind::Transport { .. }
            | ErrorKind::Preview { .. }
            | ErrorKind::Cursor
            | ErrorKind::Invalid { .. }
            | ErrorKind::Query { .. }
            | ErrorKind::Stored { .. }
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
            | ErrorKind::NameExists
            | ErrorKind::Policy
            | ErrorKind::Fence
            | ErrorKind::State { .. }
            | ErrorKind::Stale { .. }
            | ErrorKind::Precondition
            | ErrorKind::Replay
            | ErrorKind::Gone
            | ErrorKind::Model
            | ErrorKind::Balance
            | ErrorKind::Runner { .. }
            | ErrorKind::Admission { .. }
            | ErrorKind::Approval { .. }
            | ErrorKind::Token
            | ErrorKind::Provider
            | ErrorKind::Unknown
            | ErrorKind::Decision { .. }
            | ErrorKind::Inference { .. }
            | ErrorKind::Sdk { .. }
            | ErrorKind::Transport { .. }
            | ErrorKind::Preview { .. }
            | ErrorKind::Cursor
            | ErrorKind::Invalid { .. }
            | ErrorKind::Query { .. }
            | ErrorKind::Stored { .. }
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
            ErrorKind::Replay => Some("accepted"),
            ErrorKind::Review { .. } | ErrorKind::Policy => Some("review_required"),
            ErrorKind::NameExists => Some("name_exists"),
            ErrorKind::Model | ErrorKind::Inference { .. } => Some("unavailable"),
            ErrorKind::Fence => Some("fenced"),
            ErrorKind::Unknown
            | ErrorKind::Transport { .. }
            | ErrorKind::Sdk { .. }
            | ErrorKind::Decision { .. } => Some("awaiting_accounting"),
            ErrorKind::Context { .. }
            | ErrorKind::Hidden
            | ErrorKind::FleetMissing
            | ErrorKind::Stale { .. }
            | ErrorKind::Precondition
            | ErrorKind::Gone
            | ErrorKind::Balance
            | ErrorKind::Runner { .. }
            | ErrorKind::Admission { .. }
            | ErrorKind::Approval { .. }
            | ErrorKind::Token
            | ErrorKind::Provider
            | ErrorKind::Preview { .. }
            | ErrorKind::Cursor
            | ErrorKind::Invalid { .. }
            | ErrorKind::Query { .. }
            | ErrorKind::Stored { .. }
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
