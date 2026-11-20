//! Private error cases and their typed causes.

#[derive(Debug, thiserror::Error)]
pub(crate) enum ErrorKind {
    #[error("the conversation working context could not be prepared")]
    Context {
        #[source]
        source: af_context::Error,
    },
    #[error("inherited schedule update failed")]
    Cron {
        #[source]
        source: afd_cron::Error,
    },
    #[error("the setup resource is unavailable")]
    Hidden,
    #[error("the shared fleet is unavailable")]
    FleetMissing,
    #[error("the fleet is archived, and archive is terminal")]
    Archived,
    #[error("the instance name already exists in this workspace")]
    NameExists,
    #[error("the native setup policy needs a fresh review")]
    Policy,
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
    #[error("the owner wallet cannot cover the setup run's bound")]
    Balance,
    #[error("the setup run's runner could not be guarded")]
    Runner {
        #[from]
        source: afd_runner::Error,
    },
    #[error("the initial runtime turn could not be recorded")]
    TurnWrite {
        #[from]
        source: afd_turns::Error,
    },
    #[error("the setup approval request could not be recorded")]
    Approval {
        #[from]
        source: afd_approval::Error,
    },
    #[error("the fleet row could not be removed")]
    Lifecycle {
        #[from]
        source: afd_fleet_lifecycle::Error,
    },
    #[error("the assembled setup context exceeds its bound")]
    Token,
    #[error("the private preview could not be published")]
    Preview {
        #[source]
        source: afd_bus::Error,
    },
    #[error("the private setup cursor does not match its query")]
    Cursor,
    #[error("the setup request is invalid")]
    Invalid { detail: &'static str },
    #[error("the setup store could not {context}")]
    Query {
        context: &'static str,
        #[source]
        source: sqlx::Error,
    },
    #[error("the setup store holds unreadable JSON")]
    Stored {
        /// serde's position and expected type; never the private text it read.
        #[source]
        source: af_core::json::Unreadable,
    },
    #[error("the setup request violates a declared bound")]
    Validation {
        #[from]
        source: garde::Report,
    },
    #[error("the native setup is invalid")]
    Definition {
        #[from]
        source: af_fleet_runtime::Error,
    },
    #[error("the owner model could not be resolved")]
    Credential {
        #[from]
        source: afd_credential::Error,
    },
    #[error("the setup datastore is unavailable")]
    Datastore {
        #[from]
        source: afd_db::Error,
    },
    #[error("a setup identifier could not be minted")]
    Identifier {
        #[from]
        source: af_core::error::Error,
    },
    #[error("setup entropy is unavailable")]
    Entropy {
        #[from]
        source: af_crypto::error::Error,
    },
    /// The biller answered with a fault its outage policy did not absorb.
    #[error("the setup chat's biller refused the request")]
    Billing {
        #[from]
        source: afd_billing::Error,
    },
}
