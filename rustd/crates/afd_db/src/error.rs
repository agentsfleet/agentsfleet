//! The one error type this crate returns, and the wire code each failure maps to.
//!
//! Same shape as `afd_crypto::Error` (`M-ERRORS-CANONICAL-STRUCTS`): a struct
//! carrying a captured backtrace and a private kind, with `is_*` accessors
//! rather than a public enum, so a new internal failure mode is not a breaking
//! change for anyone matching on it.
//!
//! # Why capacity and unreachable stay apart
//!
//! A client cannot act on the difference and both answer `UZ-INTERNAL-001` on
//! the wire. An operator acts on nothing else: a pool that timed out waiting
//! for a free connection means the ceiling is too low or a query is too slow,
//! while a refused connection means Postgres is gone. Collapsing them is how a
//! capacity incident gets diagnosed as an outage for twenty minutes, so
//! [`Error::is_pool_capacity`] and [`Error::is_datastore_unavailable`] are two
//! questions and `sqlx::Error::PoolTimedOut` is what separates them.

use std::backtrace::Backtrace;
use std::fmt::{self, Display, Formatter};

use afd_core::error_code::{self, ErrorCode};

use crate::sql::SplitError;

/// A datastore operation failed, or the configuration for one was malformed.
///
/// One pointer wide. `afd_crypto::Error` holds its kind inline because its
/// kinds are a few words each; here the largest carries a `sqlx::Error`, which
/// is 128 bytes on its own, and this type is the `Err` of `Result`s the request
/// path returns. Boxing keeps the success path — the one that runs — the size
/// of what it actually carries (`clippy::result_large_err`).
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
    #[error("{knob} is not set")]
    MissingDatabaseUrl { knob: &'static str },

    #[error("{knob} is not a Postgres connection URL")]
    InvalidDatabaseUrl {
        knob: &'static str,
        // Boxed because `sqlx::Error` is large and this variant is the cold
        // path; an unboxed one would widen every `Result<_, Error>` this crate
        // returns, including the ones on the request path.
        #[source]
        source: Box<sqlx::Error>,
    },

    #[error("{knob} must be a postgres:// or postgresql:// URL")]
    InvalidDatabaseUrlScheme { knob: &'static str },

    #[error("{knob} must be true, false, 1 or 0")]
    InvalidBoolKnob { knob: &'static str },

    #[error("waited {waited_ms}ms for a {role} connection and the pool had none")]
    PoolCapacity { role: &'static str, waited_ms: u128 },

    #[error("the {role} datastore did not answer within {waited_ms}ms")]
    DatastoreUnreachable { role: &'static str, waited_ms: u128 },

    #[error("the {role} datastore is unreachable")]
    DatastoreUnavailable {
        role: &'static str,
        #[source]
        source: sqlx::Error,
    },

    #[error("statement failed during {context}")]
    Query {
        context: &'static str,
        #[source]
        source: sqlx::Error,
    },

    #[error("migration {version} is not valid SQL")]
    MigrationSql {
        version: i32,
        #[source]
        source: SplitError,
    },

    #[error("migration {version} failed to apply")]
    MigrationFailed {
        version: i32,
        #[source]
        source: sqlx::Error,
    },

    #[error("the migration lock was held by another session for {waited_ms}ms")]
    MigrationLockUnavailable { waited_ms: u128 },

    #[error("the ledger records version {found}, which this binary does not know")]
    MigrationSchemaAhead { found: i32 },
}

impl Error {
    pub(crate) fn new(kind: ErrorKind) -> Self {
        Self {
            inner: Box::new(Inner {
                kind,
                backtrace: Backtrace::capture(),
            }),
        }
    }

    /// Whether a role's connection URL was absent or malformed.
    #[must_use]
    pub fn is_config(&self) -> bool {
        matches!(
            self.inner.kind,
            ErrorKind::MissingDatabaseUrl { .. }
                | ErrorKind::InvalidDatabaseUrl { .. }
                | ErrorKind::InvalidDatabaseUrlScheme { .. }
                | ErrorKind::InvalidBoolKnob { .. }
        )
    }

    /// Whether the pool ran out of connections before the acquire timeout.
    ///
    /// The datastore is up; there was nothing free to hand out. See the module
    /// documentation for why this is not folded into the next question.
    #[must_use]
    pub fn is_pool_capacity(&self) -> bool {
        matches!(self.inner.kind, ErrorKind::PoolCapacity { .. })
    }

    /// Whether Postgres could not be reached at all.
    #[must_use]
    pub fn is_datastore_unavailable(&self) -> bool {
        matches!(
            self.inner.kind,
            ErrorKind::DatastoreUnavailable { .. } | ErrorKind::DatastoreUnreachable { .. }
        )
    }

    /// Whether a statement reached Postgres and Postgres refused it.
    #[must_use]
    pub fn is_query(&self) -> bool {
        matches!(self.inner.kind, ErrorKind::Query { .. })
    }

    /// Whether a migration could not be applied, for any reason of its own.
    #[must_use]
    pub fn is_migration_failed(&self) -> bool {
        matches!(
            self.inner.kind,
            ErrorKind::MigrationSql { .. } | ErrorKind::MigrationFailed { .. }
        )
    }

    /// Whether the ledger refused this binary — lock held, or a version ahead.
    #[must_use]
    pub fn is_migration_refused(&self) -> bool {
        matches!(
            self.inner.kind,
            ErrorKind::MigrationLockUnavailable { .. } | ErrorKind::MigrationSchemaAhead { .. }
        )
    }

    /// The registry code a handler would surface for this failure.
    #[must_use]
    pub fn code(&self) -> ErrorCode {
        match self.inner.kind {
            ErrorKind::PoolCapacity { .. }
            | ErrorKind::DatastoreUnavailable { .. }
            | ErrorKind::DatastoreUnreachable { .. }
            | ErrorKind::MissingDatabaseUrl { .. }
            | ErrorKind::InvalidDatabaseUrl { .. }
            | ErrorKind::InvalidDatabaseUrlScheme { .. }
            | ErrorKind::InvalidBoolKnob { .. } => error_code::INTERNAL_DB_UNAVAILABLE,
            ErrorKind::Query { .. } => error_code::INTERNAL_DB_QUERY,
            ErrorKind::MigrationSql { .. }
            | ErrorKind::MigrationFailed { .. }
            | ErrorKind::MigrationLockUnavailable { .. }
            | ErrorKind::MigrationSchemaAhead { .. } => error_code::STARTUP_MIGRATION_CHECK,
        }
    }

    /// The backtrace captured when this error was constructed.
    ///
    /// Empty unless `RUST_BACKTRACE` asked for one — capturing is opt-in, so
    /// the common path costs a few instructions rather than microseconds.
    pub fn backtrace(&self) -> &Backtrace {
        &self.inner.backtrace
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
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.inner.kind)
    }
}

/// Classifies a failed acquire, which is the one place the two operational
/// stories separate.
///
/// `sqlx` reports the capacity case as `PoolTimedOut` and everything else —
/// refused connection, DNS failure, TLS handshake, an authentication rejection
/// — as itself. `waited_ms` is the configured acquire timeout rather than a
/// measured duration: it is what the operator set, which is the number they
/// need when deciding whether to raise it.
pub(crate) fn classify_acquire(role: &'static str, waited_ms: u128, source: sqlx::Error) -> Error {
    match source {
        sqlx::Error::PoolTimedOut => Error::new(ErrorKind::PoolCapacity { role, waited_ms }),
        other => Error::new(ErrorKind::DatastoreUnavailable {
            role,
            source: other,
        }),
    }
}

/// Wraps a statement failure with the operation that issued it.
pub(crate) fn query(context: &'static str, source: sqlx::Error) -> Error {
    Error::new(ErrorKind::Query { context, source })
}

/// Builds the malformed-boolean-knob error, which callers outside this module
/// need because the knobs they read are theirs, not this module's.
#[must_use]
pub fn invalid_bool_knob(knob: &'static str) -> Error {
    Error::new(ErrorKind::InvalidBoolKnob { knob })
}

/// A datastore that never answered, reported as the outage it is.
///
/// The timeout path has no `sqlx::Error` of its own to carry — nothing came
/// back — so the variant is built directly rather than classified.
pub(crate) fn unreachable_datastore(role: &'static str, waited_ms: u128) -> Error {
    Error::new(ErrorKind::DatastoreUnreachable { role, waited_ms })
}
