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
    #[error("the setup edit decision could not complete")]
    Decision {
        #[source]
        source: af_providers::Error,
    },
    #[error("the configured model settings are invalid")]
    Inference {
        #[source]
        source: af_inference::Error,
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
    #[error("the private preview could not be published")]
    Preview {
        #[source]
        source: afd_bus::Error,
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
        source: af_core::json::Unreadable,
    },
    #[error("the authoring request violates a declared bound")]
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
    #[error("the authoring datastore is unavailable")]
    Datastore {
        #[from]
        source: afd_db::Error,
    },
    #[error("an authoring identifier could not be minted")]
    Identifier {
        #[from]
        source: af_core::error::Error,
    },
    #[error("authoring entropy is unavailable")]
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
