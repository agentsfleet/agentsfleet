//! Contextual constructors for tenant failures.

use super::{ApiKeyField, Error, ErrorKind, SessionField};

/// Reports a statement that failed, naming what it was doing.
pub(crate) fn query(context: &'static str) -> impl Fn(sqlx::Error) -> Error {
    move |source| Error::new(ErrorKind::Query { context, source })
}

/// Reports a column whose stored value is not a shape this daemon can read.
pub(crate) fn row_malformed(
    table: &'static str,
    column: &'static str,
) -> impl Fn(afd_core::error::Error) -> Error {
    move |source| {
        Error::new(ErrorKind::RowMalformed {
            table,
            column,
            source,
        })
    }
}

/// Refuses a device-flow field this daemon will not store.
pub(crate) fn session_field(field: SessionField) -> Error {
    Error::new(ErrorKind::SessionFieldInvalid { field })
}

/// Reports a session id naming nothing this daemon holds.
pub(crate) fn session_missing() -> Error {
    Error::new(ErrorKind::SessionMissing)
}

/// Reports a session past its expiry.
pub(crate) fn session_expired() -> Error {
    Error::new(ErrorKind::SessionExpired)
}

/// Reports a session whose credential was already collected.
pub(crate) fn session_consumed() -> Error {
    Error::new(ErrorKind::SessionConsumed)
}

/// Reports a session somebody cancelled.
pub(crate) fn session_aborted() -> Error {
    Error::new(ErrorKind::SessionAborted)
}

/// Reports a session aborted by its own attempt ceiling.
pub(crate) fn session_rate_limited() -> Error {
    Error::new(ErrorKind::SessionRateLimited)
}

/// Reports a verify against a session nobody has approved.
pub(crate) fn session_not_approved() -> Error {
    Error::new(ErrorKind::SessionNotApproved)
}

/// Reports a second approval of one session.
pub(crate) fn session_already_approved() -> Error {
    Error::new(ErrorKind::SessionAlreadyApproved)
}

/// Reports a verification code that did not match.
pub(crate) fn session_code_rejected() -> Error {
    Error::new(ErrorKind::SessionCodeRejected)
}

/// Reports a caller acting on a session that is not theirs.
pub(crate) fn session_not_owner() -> Error {
    Error::new(ErrorKind::SessionNotOwner)
}

/// Refuses an api-key field this daemon will not store.
pub(crate) fn apikey_field(field: ApiKeyField) -> Error {
    Error::new(ErrorKind::ApiKeyFieldInvalid { field })
}

/// Reports an id naming no key this tenant holds.
pub(crate) fn apikey_not_found() -> Error {
    Error::new(ErrorKind::ApiKeyNotFound)
}

/// Reports a name this tenant already uses.
pub(crate) fn apikey_name_taken() -> Error {
    Error::new(ErrorKind::ApiKeyNameTaken)
}

/// Reports a revoke of a key that was already revoked.
pub(crate) fn apikey_already_revoked() -> Error {
    Error::new(ErrorKind::ApiKeyAlreadyRevoked)
}

/// Refuses a patch that would bring a revoked key back.
pub(crate) fn apikey_readonly_field() -> Error {
    Error::new(ErrorKind::ApiKeyReadonlyField)
}

/// Refuses a delete of a key that is still live.
pub(crate) fn apikey_must_revoke_first() -> Error {
    Error::new(ErrorKind::ApiKeyMustRevokeFirst)
}

/// Refuses a machine name this daemon will not store.
pub(crate) fn cli_credential_machine_name() -> Error {
    Error::new(ErrorKind::CliCredentialMachineNameInvalid)
}

/// Reports an id naming no live credential this user holds.
pub(crate) fn cli_credential_not_found() -> Error {
    Error::new(ErrorKind::CliCredentialNotFound)
}

/// Refuses a workspace name carrying a character this daemon will not store.
pub(crate) fn workspace_name_invalid() -> Error {
    Error::new(ErrorKind::WorkspaceNameInvalid)
}

/// Refuses a workspace name past the length cap.
pub(crate) fn workspace_name_too_long() -> Error {
    Error::new(ErrorKind::WorkspaceNameTooLong)
}

/// Refuses a workspace name this tenant already uses.
pub(crate) fn workspace_name_exists() -> Error {
    Error::new(ErrorKind::WorkspaceNameExists)
}

/// Refuses a create whose session names a tenant with no row behind it.
pub(crate) fn workspace_tenant_vanished() -> Error {
    Error::new(ErrorKind::WorkspaceTenantVanished)
}

/// Reports a catalogue page statement that would not answer.
pub(crate) fn library_page_unavailable(source: sqlx::Error) -> Error {
    Error::new(ErrorKind::LibraryPageUnavailable { source })
}

/// Reports an insert the machine's unique index refused.
///
/// Not a refusal a caller ever sees on the first attempt: [`crate::
/// cli_credential::CliCredentials::mint`] retries on it, because it means
/// another login for this machine committed first and a second pass will
/// revoke that row and take its place.
pub(crate) fn cli_credential_machine_collision() -> Error {
    Error::new(ErrorKind::CliCredentialMachineCollision)
}

/// Reports a proven subject with no `core.users` row behind it.
pub(crate) fn cli_credential_unknown_subject() -> Error {
    Error::new(ErrorKind::CliCredentialUnknownSubject)
}
