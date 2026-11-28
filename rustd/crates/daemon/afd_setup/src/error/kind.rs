//! Private error cases and their typed causes.

#[derive(Debug, thiserror::Error)]
pub(crate) enum ErrorKind {
    #[error("the caller is not a current member of the workspace")]
    Hidden,
    #[error("the fleet is unavailable")]
    FleetMissing,
    #[error("the fleet is archived, and archive is terminal")]
    Archived,
    #[error("the instance name already exists in this workspace")]
    NameExists,
    #[error("the fleet's resource version changed")]
    Stale { etag: String },
    #[error("the change omitted its version")]
    Precondition,
    #[error("the operation key belongs to another request")]
    Replay,
    #[error("the fleet the operation created was deleted")]
    Gone,
    #[error("the requested or selected model is unavailable")]
    Model,
    #[error("the selected model cannot receive an attached image")]
    ImageModel { model: String },
    #[error("the first message could not be recorded")]
    TurnWrite {
        #[from]
        source: afd_turns::Error,
    },
    #[error("the fleet's grants could not be read")]
    Approval {
        #[from]
        source: afd_approval::Error,
    },
    #[error("the fleet row could not be removed")]
    Lifecycle {
        #[from]
        source: afd_fleet_lifecycle::Error,
    },
    #[error("the continuation does not match its query")]
    Cursor,
    #[error("the request is invalid")]
    Invalid { detail: &'static str },
    #[error("the fleet store could not {context}")]
    Query {
        context: &'static str,
        #[source]
        source: sqlx::Error,
    },
    #[error("the fleet store holds unreadable JSON")]
    Stored {
        /// serde's position and expected type; never the private text it read.
        #[source]
        source: af_core::json::Unreadable,
    },
    #[error("the request violates a declared bound")]
    Validation {
        #[from]
        source: garde::Report,
    },
    #[error("the fleet name is invalid")]
    Name {
        #[from]
        source: af_fleet_runtime::Error,
    },
    #[error("the workspace model could not be resolved")]
    Credential {
        #[from]
        source: afd_credential::Error,
    },
    #[error("the fleet datastore is unavailable")]
    Datastore {
        #[from]
        source: afd_db::Error,
    },
    #[error("an identifier could not be minted or read")]
    Identifier {
        #[from]
        source: af_core::error::Error,
    },
    #[error("entropy is unavailable")]
    Entropy {
        #[from]
        source: af_crypto::error::Error,
    },
}
