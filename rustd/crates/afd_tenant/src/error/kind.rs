//! Private failure vocabulary owned by the tenant crate.

use super::{ApiKeyField, SessionField};

/// What actually went wrong. Private so a new variant is not a breaking change.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ErrorKind {
    #[error("the datastore backing the tenant plane would not answer")]
    Datastore {
        #[source]
        source: afd_db::Error,
    },

    #[error("the queue backing the tenant plane would not answer")]
    Queue {
        #[source]
        source: afd_redis::Error,
    },

    #[error("statement failed during {context}")]
    Query {
        context: &'static str,
        #[source]
        source: sqlx::Error,
    },

    #[error("a {table} row holds a value this daemon cannot read: {column}")]
    RowMalformed {
        table: &'static str,
        column: &'static str,
        #[source]
        source: afd_core::error::Error,
    },

    #[error("an identifier could not be minted from the current instant")]
    Mint {
        #[from]
        source: afd_core::error::Error,
    },

    #[error("could not draw the entropy a credential is minted from")]
    Entropy {
        #[source]
        source: afd_crypto::error::Error,
    },

    #[error("a device-flow login field was refused: {field}")]
    SessionFieldInvalid { field: SessionField },

    #[error("no device-flow login session is held under that id")]
    SessionMissing,

    #[error("the device-flow login session's window closed before it was redeemed")]
    SessionExpired,

    #[error("the device-flow login session was already redeemed")]
    SessionConsumed,

    #[error("the device-flow login session was cancelled, superseded, or rate-limited")]
    SessionAborted,

    #[error("the device-flow login session was aborted by its own attempt ceiling")]
    SessionRateLimited,

    #[error("no human has approved this device-flow login session yet")]
    SessionNotApproved,

    #[error("this device-flow login session is already past pending")]
    SessionAlreadyApproved,

    #[error("the presented code did not match the session's stored digest")]
    SessionCodeRejected,

    #[error("this device-flow login session belongs to another identity")]
    SessionNotOwner,

    #[error("an api-key field was refused: {field}")]
    ApiKeyFieldInvalid { field: ApiKeyField },

    #[error("no api-key with that id belongs to this tenant")]
    ApiKeyNotFound,

    #[error("this tenant already holds an api-key under that name")]
    ApiKeyNameTaken,

    #[error("this api-key was already revoked, so nothing changed")]
    ApiKeyAlreadyRevoked,

    #[error("an api-key cannot be brought back once revoked")]
    ApiKeyReadonlyField,

    #[error("an active api-key must be revoked before it can be deleted")]
    ApiKeyMustRevokeFirst,

    #[error("a machine name was refused")]
    CliCredentialMachineNameInvalid,

    #[error("no live command-line credential with that id belongs to this user")]
    CliCredentialNotFound,

    #[error("another login for this machine committed first")]
    CliCredentialMachineCollision,

    #[error("the authenticated subject has no user record")]
    CliCredentialUnknownSubject,

    #[error("a workspace name carries a character this daemon will not store")]
    WorkspaceNameInvalid,

    #[error("a workspace name is past the length cap")]
    WorkspaceNameTooLong,

    #[error("this tenant already holds a workspace under that name")]
    WorkspaceNameExists,

    #[error("the session's tenant claim names no tenant row")]
    WorkspaceTenantVanished,

    #[error("the catalogue page statement would not answer")]
    LibraryPageUnavailable {
        #[source]
        source: sqlx::Error,
    },
}
