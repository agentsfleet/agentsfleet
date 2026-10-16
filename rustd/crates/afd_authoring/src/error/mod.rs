//! One error hull; private kinds preserve their causes and safe public meaning.

use afd_core::error_code::{self, ErrorCode};

pub(crate) mod raise;
#[cfg(test)]
mod tests;

afd_core::error_shell!(
    /// A private-setup refusal or a failure to reach its durable store.
    pub struct Error(ErrorKind);
);

/// The result every fallible authoring operation returns.
pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub(crate) enum ErrorKind {
    #[error("inherited schedule update failed")]
    Cron {
        #[source]
        source: afd_cron::Error,
    },
    #[error("the setup resource is unavailable")]
    Hidden,
    #[error("the shared fleet is unavailable")]
    FleetMissing,
    #[error("the instance name already exists in this workspace")]
    NameExists,
    #[error("the native setup policy needs a fresh review")]
    Policy,
    #[error("the authoring attempt cannot publish")]
    Fence,
    #[error("the setup state {current} prevents the mutation")]
    State { current: &'static str },
    #[error("the setup resource version changed")]
    Stale { etag: String },
    #[error("the setup mutation omitted its version")]
    Precondition,
    #[error("the operation key belongs to another normalized request")]
    Replay,
    #[error("the effective setup requires a fresh human review")]
    Review { digest: String },
    #[error("the accepted setup resource was purged")]
    Gone,
    #[error("the workspace owner's selected model is unavailable")]
    Model,
    #[error("the owner wallet cannot cover the authoring bound")]
    Balance,
    #[error("the authoring host could not be guarded")]
    Runner {
        #[from]
        source: afd_runner::Error,
    },
    #[error("the initial runtime admission could not be recorded")]
    Admission {
        #[from]
        source: afd_admission::Error,
    },
    #[error("the setup approval request could not be recorded")]
    Approval {
        #[from]
        source: afd_approval::Error,
    },
    #[error("the assembled authoring context exceeds its bound")]
    Token,
    #[error("the provider failed with known usage")]
    Provider,
    #[error("the provider obligation remains unresolved")]
    Unknown,
    #[error("the configured model settings are invalid")]
    Inference {
        #[source]
        source: afd_inference::Error,
    },
    #[error("the model SDK could not complete the dispatch")]
    Sdk {
        #[source]
        source: rig_core::ProviderError,
    },
    #[error("the authoring transport could not complete")]
    Transport {
        #[from]
        source: reqwest::Error,
    },
    /// Absent cause: the queue did not answer within its bound.
    #[error("the fleet stream could not be created, so the install was removed")]
    RolledBack {
        #[source]
        source: Option<afd_dragonfly::Error>,
    },
    #[error("the private setup cursor does not match its query")]
    Cursor,
    #[error("the setup request is invalid")]
    Invalid { detail: &'static str },
    #[error("the authoring store could not {context}")]
    Query {
        context: &'static str,
        #[source]
        source: sqlx::Error,
    },
    #[error("the authoring store holds unreadable JSON")]
    Stored {
        /// serde's position and expected type; never the private text it read.
        #[source]
        source: afd_core::json::Unreadable,
    },
    #[error("the authoring request violates a declared bound")]
    Validation {
        #[from]
        source: garde::Report,
    },
    #[error("the native setup is invalid")]
    Definition {
        #[from]
        source: afd_fleet_runtime::Error,
    },
    #[error("the owner model could not be resolved")]
    Credential {
        #[from]
        source: afd_credential::Error,
    },
    #[error("the authoring datastore is unavailable")]
    Datastore {
        #[from]
        source: afd_db::Error,
    },
    #[error("an authoring identifier could not be minted")]
    Identifier {
        #[from]
        source: afd_core::error::Error,
    },
    #[error("authoring entropy is unavailable")]
    Entropy {
        #[from]
        source: afd_crypto::error::Error,
    },
}

afd_core::error_lifts!(Error, ErrorKind:
    afd_cron::Error => Cron,
    afd_db::Error => Datastore,
    afd_credential::Error => Credential,
    afd_fleet_runtime::Error => Definition,
    afd_crypto::error::Error => Entropy,
    afd_core::error::Error => Identifier,
    garde::Report => Validation,
    reqwest::Error => Transport,
    afd_inference::Error => Inference,
    rig_core::ProviderError => Sdk,
    afd_runner::Error => Runner,
    afd_admission::Error => Admission,
    afd_approval::Error => Approval,
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
            ErrorKind::Provider => error_code::AUTHORING_PROVIDER,
            ErrorKind::Unknown | ErrorKind::Transport { .. } | ErrorKind::Sdk { .. } => {
                error_code::AUTHORING_UNKNOWN
            }
            ErrorKind::RolledBack { .. } => error_code::AGENTSFLEET_INSTALL_ROLLED_BACK,
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
            ErrorKind::Identifier { .. } | ErrorKind::Entropy { .. } => {
                error_code::INTERNAL_OPERATION_FAILED
            }
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
            _ => afd_core::problem::Problem::of(self.code()).title(),
        }
    }

    /// The current tag required on a precondition refusal.
    #[must_use]
    pub fn etag(&self) -> Option<&str> {
        match self.kind() {
            ErrorKind::Cron { source } => source.etag(),
            ErrorKind::Stale { etag } => Some(etag),
            _ => None,
        }
    }

    /// Fresh daemon-computed digest, carried only by a stale-review refusal.
    #[must_use]
    pub fn review_digest(&self) -> Option<&str> {
        match self.kind() {
            ErrorKind::Review { digest } => Some(digest),
            _ => None,
        }
    }

    /// The bounded state required on a lifecycle conflict.
    #[must_use]
    pub fn current_state(&self) -> Option<&'static str> {
        match self.kind() {
            ErrorKind::Cron { source } => source.current_state(),
            ErrorKind::State { current } => Some(current),
            ErrorKind::Replay => Some("accepted"),
            ErrorKind::Review { .. } | ErrorKind::Policy => Some("review_required"),
            ErrorKind::NameExists => Some("name_exists"),
            ErrorKind::Model | ErrorKind::Inference { .. } => Some("unavailable"),
            ErrorKind::Fence => Some("fenced"),
            ErrorKind::Unknown | ErrorKind::Transport { .. } | ErrorKind::Sdk { .. } => {
                Some("awaiting_accounting")
            }
            _ => None,
        }
    }

    /// Whether retrying after a datastore outage may succeed.
    #[must_use]
    pub fn is_datastore_unavailable(&self) -> bool {
        self.code() == error_code::INTERNAL_DB_UNAVAILABLE
    }
}
