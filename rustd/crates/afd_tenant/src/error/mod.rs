//! The one error type this crate returns, and the wire code each failure maps to.
//!
//! Same shape as [`afd_db::Error`] and [`afd_fleet::Error`]
//! (`M-ERRORS-CANONICAL-STRUCTS`): a struct carrying a captured backtrace and a
//! private kind, with `is_*` accessors rather than a public enum, so a new
//! internal failure mode is not a breaking change for anyone matching on it.
//!
//! # Why this crate has its own, rather than borrowing the fleet crate's
//!
//! `RUST_ERROR_STANDARD` asks for one error type per crate, and the reason is
//! visible here: the failures below are the tenant plane's whole vocabulary —
//! a name already taken, a login already approved, a credential that must be
//! revoked before it is deleted. None of them can be raised by a lease, a gate
//! or a budget drain, and none of the runner plane's can be raised here. A
//! shared enum would let each side match on arms the other produces, which is
//! exactly the coupling splitting the crates was for.

use std::backtrace::Backtrace;
use std::fmt::{self, Display, Formatter};

use afd_core::error_code::{self, ErrorCode};

pub mod detail;

pub use self::detail::*;

/// The result every fallible function in this crate returns.
///
/// One alias per crate, defaulted to that crate's own [`Error`] — the shape
/// `core_api` has run in production on for years. The point is not brevity: it
/// is that a reader never has to check WHICH error a signature returns to know
/// it is this crate's.
pub type Result<T, E = Error> = core::result::Result<T, E>;

/// A tenant-plane failure, with the backtrace of where it was raised.
#[derive(Debug)]
pub struct Error {
    inner: Box<Inner>,
}

#[derive(Debug)]
struct Inner {
    kind: ErrorKind,
    backtrace: Backtrace,
}

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

    #[error("a tenant reached billing with no wallet row behind it")]
    BillingWalletMissing,

    #[error("a charges cursor this daemon never issued")]
    ChargesCursorInvalid,

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

/// Which device-flow field a refusal names.
///
/// One kind with a field rather than five kinds, because the five differ in
/// exactly one way — the code and sentence they answer with — and a table
/// keyed on the field says that once. The Zig store spells five error tags and
/// `failFromStoreError` re-pairs each with its code and its sentence at a
/// switch arm, which is the same fact written three times.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionField {
    /// The command line's public key, presented at creation.
    PublicKey,
    /// The label the minted credential will carry.
    TokenName,
    /// The relayed envelope the dashboard sealed.
    Ciphertext,
    /// The nonce that envelope was sealed under.
    Nonce,
    /// The six digits a person reads out of the browser.
    VerificationCode,
}

impl Display for SessionField {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::PublicKey => "public_key",
            Self::TokenName => "token_name",
            Self::Ciphertext => "ciphertext",
            Self::Nonce => "nonce",
            Self::VerificationCode => "verification_code",
        })
    }
}

/// Which api-key field a refusal names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiKeyField {
    /// The name the key is listed and grepped under.
    Name,
    /// The free text beside it.
    Description,
}

impl Display for ApiKeyField {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Name => "key_name",
            Self::Description => "description",
        })
    }
}

impl Error {
    /// Wraps `kind`, capturing a backtrace if one was asked for.
    pub(crate) fn new(kind: ErrorKind) -> Self {
        Self {
            inner: Box::new(Inner {
                kind,
                backtrace: Backtrace::capture(),
            }),
        }
    }

    /// The backtrace captured when this error was constructed.
    ///
    /// Empty unless `RUST_BACKTRACE` asked for one — capturing is opt-in, so
    /// the common path costs a few instructions rather than microseconds.
    pub fn backtrace(&self) -> &Backtrace {
        &self.inner.backtrace
    }

    /// Whether this is a lost race for one machine's live credential.
    ///
    /// The mint asks, and retries once when it is true. Crate-private: a lost
    /// race is that module's business to resolve, not something a caller should
    /// be branching on.
    pub(crate) const fn is_machine_collision(&self) -> bool {
        matches!(self.inner.kind, ErrorKind::CliCredentialMachineCollision)
    }

    /// Whether the datastore behind this plane could not be reached.
    ///
    /// The question the HTTP edge turns on: an outage is this instance's
    /// problem to report as a 503, where every other failure here is the
    /// caller's to correct.
    #[must_use]
    pub const fn is_datastore_unavailable(&self) -> bool {
        matches!(
            self.inner.kind,
            ErrorKind::Datastore { .. }
                | ErrorKind::Queue { .. }
                | ErrorKind::LibraryPageUnavailable { .. }
        )
    }

    /// The registry code this failure answers with.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        match self.inner.kind {
            // One code for both stores. A caller acts identically on either —
            // this instance could not reach what it needed — and which of the
            // two it was belongs in the log beside the request id, not in a
            // code a dashboard would have to branch on.
            ErrorKind::Datastore { .. } | ErrorKind::Queue { .. } => {
                error_code::INTERNAL_DB_UNAVAILABLE
            }
            ErrorKind::Query { .. } | ErrorKind::RowMalformed { .. } => {
                error_code::INTERNAL_DB_QUERY
            }
            // One internal code for four failures the caller shares an
            // inability to correct. Two are this instance's own — a mint that
            // failed, a host that cannot draw entropy. A missing wallet is
            // operator surgery or a defect, because signup bootstrap writes it
            // in the tenant-create transaction; a machine collision reaching
            // the edge means the mint's retry already lost twice in a row. The
            // SENTENCES separate them where separation helps.
            ErrorKind::Mint { .. }
            | ErrorKind::Entropy { .. }
            | ErrorKind::BillingWalletMissing
            | ErrorKind::CliCredentialMachineCollision => error_code::INTERNAL_OPERATION_FAILED,
            ErrorKind::SessionFieldInvalid { field } => match field {
                SessionField::PublicKey => error_code::INVALID_PUBLIC_KEY,
                SessionField::TokenName => error_code::INVALID_TOKEN_NAME,
                SessionField::Ciphertext => error_code::INVALID_CIPHERTEXT,
                SessionField::Nonce => error_code::INVALID_NONCE,
                SessionField::VerificationCode => error_code::INVALID_VERIFICATION_CODE,
            },
            ErrorKind::SessionMissing => error_code::SESSION_NOT_FOUND,
            ErrorKind::SessionExpired => error_code::SESSION_EXPIRED,
            ErrorKind::SessionConsumed => error_code::SESSION_CONSUMED,
            // One code for both, and the sentences differ rather than the
            // codes: a client acts identically on either — stop, log in again —
            // and the ceiling is the only one of the two worth naming in prose.
            ErrorKind::SessionAborted | ErrorKind::SessionRateLimited => {
                error_code::SESSION_ABORTED
            }
            ErrorKind::SessionNotApproved => error_code::SESSION_NOT_APPROVED,
            ErrorKind::SessionAlreadyApproved => error_code::SESSION_ALREADY_APPROVED,
            ErrorKind::SessionCodeRejected => error_code::VERIFICATION_FAILED,
            // One code for two refusals from different families, and the
            // SENTENCES are what separate them: a caller who does not own a
            // login session and a subject with no user row are both told they
            // may not proceed, and neither can fix it by re-authenticating.
            ErrorKind::SessionNotOwner | ErrorKind::CliCredentialUnknownSubject => {
                error_code::AUTH_FORBIDDEN
            }
            ErrorKind::ApiKeyFieldInvalid { .. }
            | ErrorKind::CliCredentialMachineNameInvalid
            | ErrorKind::ChargesCursorInvalid
            | ErrorKind::WorkspaceNameInvalid
            | ErrorKind::WorkspaceNameTooLong => error_code::INVALID_REQUEST,
            ErrorKind::WorkspaceNameExists => error_code::WORKSPACE_NAME_EXISTS,
            // A 401 and not a 403: the session's tenant is GONE, so the
            // credential itself is stale and re-authenticating is the remedy.
            ErrorKind::WorkspaceTenantVanished => error_code::AUTH_UNAUTHORIZED,
            // The library family's own transient code, not INTERNAL-001: the
            // catalogue read has carried `UZ-LIBRARY-006` since it shipped,
            // and a dashboard's retry logic may already branch on it.
            ErrorKind::LibraryPageUnavailable { .. } => error_code::LIBRARY_DB_UNAVAILABLE,
            ErrorKind::ApiKeyNotFound => error_code::APIKEY_NOT_FOUND,
            ErrorKind::ApiKeyNameTaken => error_code::APIKEY_NAME_TAKEN,
            ErrorKind::ApiKeyAlreadyRevoked => error_code::APIKEY_ALREADY_REVOKED,
            ErrorKind::ApiKeyReadonlyField => error_code::APIKEY_READONLY_FIELD,
            ErrorKind::ApiKeyMustRevokeFirst => error_code::APIKEY_MUST_REVOKE_FIRST,
            ErrorKind::CliCredentialNotFound => error_code::AUTH_CLI_CREDENTIAL_NOT_FOUND,
        }
    }

    /// The sentence the caller is told.
    ///
    /// A rejection quotes its own detail, because the caller can act on it.
    /// Every internal failure answers a FIXED sentence: one that quoted its
    /// cause would be leaking that cause to whoever provoked it, and the cause
    /// is in the log beside the request id.
    #[must_use]
    pub const fn detail(&self) -> &'static str {
        match self.inner.kind {
            ErrorKind::Datastore { .. } => DETAIL_DATABASE_UNAVAILABLE,
            ErrorKind::Queue { .. } => DETAIL_QUEUE_UNAVAILABLE,
            // Every internal failure answers ONE fixed sentence, deliberately:
            // a statement that failed, a row this daemon cannot read, a clock
            // that cannot name an instant and a host that cannot draw random
            // bytes are all this process's problem, and telling the caller
            // which would be leaking the cause to whoever provoked it. The
            // cause is in the log beside the request id.
            ErrorKind::Query { .. }
            | ErrorKind::RowMalformed { .. }
            | ErrorKind::Mint { .. }
            | ErrorKind::Entropy { .. }
            | ErrorKind::CliCredentialMachineCollision => DETAIL_DATABASE_ERROR,
            ErrorKind::SessionFieldInvalid { field } => match field {
                SessionField::PublicKey => DETAIL_SESSION_PUBLIC_KEY,
                SessionField::TokenName => DETAIL_SESSION_TOKEN_NAME,
                SessionField::Ciphertext => DETAIL_SESSION_CIPHERTEXT,
                SessionField::Nonce => DETAIL_SESSION_NONCE,
                SessionField::VerificationCode => DETAIL_SESSION_CODE_SHAPE,
            },
            ErrorKind::SessionMissing => DETAIL_SESSION_MISSING,
            ErrorKind::SessionExpired => DETAIL_SESSION_EXPIRED,
            ErrorKind::SessionConsumed => DETAIL_SESSION_CONSUMED,
            ErrorKind::SessionAborted => DETAIL_SESSION_ABORTED,
            ErrorKind::SessionRateLimited => DETAIL_SESSION_RATE_LIMITED,
            ErrorKind::SessionNotApproved => DETAIL_SESSION_NOT_APPROVED,
            ErrorKind::SessionAlreadyApproved => DETAIL_SESSION_ALREADY_APPROVED,
            ErrorKind::SessionCodeRejected => DETAIL_SESSION_CODE_REJECTED,
            ErrorKind::SessionNotOwner => DETAIL_SESSION_NOT_OWNER,
            ErrorKind::ApiKeyFieldInvalid { field } => match field {
                ApiKeyField::Name => DETAIL_APIKEY_NAME,
                ApiKeyField::Description => DETAIL_APIKEY_DESCRIPTION,
            },
            ErrorKind::ApiKeyNotFound => DETAIL_APIKEY_NOT_FOUND,
            ErrorKind::ApiKeyNameTaken => DETAIL_APIKEY_NAME_TAKEN,
            ErrorKind::ApiKeyAlreadyRevoked => DETAIL_APIKEY_ALREADY_REVOKED,
            ErrorKind::ApiKeyReadonlyField => DETAIL_APIKEY_READONLY_FIELD,
            ErrorKind::ApiKeyMustRevokeFirst => DETAIL_APIKEY_MUST_REVOKE_FIRST,
            ErrorKind::CliCredentialMachineNameInvalid => DETAIL_CLI_CREDENTIAL_MACHINE_NAME,
            ErrorKind::CliCredentialNotFound => DETAIL_CLI_CREDENTIAL_NOT_FOUND,
            ErrorKind::CliCredentialUnknownSubject => DETAIL_CLI_CREDENTIAL_UNKNOWN_SUBJECT,
            ErrorKind::BillingWalletMissing => DETAIL_BILLING_WALLET_MISSING,
            ErrorKind::ChargesCursorInvalid => DETAIL_CHARGES_CURSOR_INVALID,
            ErrorKind::WorkspaceNameInvalid => DETAIL_WORKSPACE_NAME_INVALID,
            ErrorKind::WorkspaceNameTooLong => DETAIL_WORKSPACE_NAME_TOO_LONG,
            ErrorKind::WorkspaceNameExists => DETAIL_WORKSPACE_NAME_EXISTS,
            ErrorKind::WorkspaceTenantVanished => DETAIL_WORKSPACE_TENANT_VANISHED,
            ErrorKind::LibraryPageUnavailable { .. } => DETAIL_LIBRARY_PAGE_UNAVAILABLE,
        }
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.code().as_str(), self.inner.kind)?;
        if self.inner.backtrace.status() == std::backtrace::BacktraceStatus::Captured {
            write!(f, "\n{}", self.inner.backtrace)?;
        }
        Ok(())
    }
}

impl std::error::Error for Error {
    /// The failure beneath this one, skipping our own kind.
    ///
    /// `Display` already renders the kind's message, so returning `&self.kind`
    /// would make a chain walker print the same sentence twice before reaching
    /// anything new. The kind is not a CAUSE of this error, it IS this error;
    /// what caused it is whatever the kind wraps (`RUST_ERROR_STANDARD` rule 4).
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        std::error::Error::source(&self.inner.kind)
    }
}

impl From<afd_db::Error> for Error {
    fn from(source: afd_db::Error) -> Self {
        Self::new(ErrorKind::Datastore { source })
    }
}

impl From<afd_redis::Error> for Error {
    fn from(source: afd_redis::Error) -> Self {
        Self::new(ErrorKind::Queue { source })
    }
}

impl From<afd_core::error::Error> for Error {
    fn from(source: afd_core::error::Error) -> Self {
        Self::new(ErrorKind::Mint { source })
    }
}

impl From<afd_crypto::error::Error> for Error {
    fn from(source: afd_crypto::error::Error) -> Self {
        Self::new(ErrorKind::Entropy { source })
    }
}

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

/// Reports a tenant whose wallet row is not there.
pub(crate) fn billing_wallet_missing() -> Error {
    Error::new(ErrorKind::BillingWalletMissing)
}

/// Refuses a charges cursor this daemon never issued.
pub(crate) fn charges_cursor_invalid() -> Error {
    Error::new(ErrorKind::ChargesCursorInvalid)
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
