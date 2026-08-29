//! The one error type returned by operator read projections.

use std::backtrace::Backtrace;
use std::fmt::{self, Display, Formatter};

use afd_core::error_code::{self, ErrorCode};

/// Result returned by every fallible operator projection.
pub type Result<T, E = Error> = core::result::Result<T, E>;

/// A fleet operator projection failed.
#[derive(Debug)]
pub struct Error {
    inner: Box<Inner>,
}

#[derive(Debug)]
struct Inner {
    kind: ErrorKind,
    backtrace: Backtrace,
}

#[derive(Debug, thiserror::Error)]
enum ErrorKind {
    #[error("the datastore backing operator views would not answer")]
    Datastore {
        #[source]
        source: afd_db::Error,
    },
    #[error("statement failed during {context}")]
    Query {
        context: &'static str,
        #[source]
        source: sqlx::Error,
    },
    #[error("a runner lease row holds an unreadable {column}")]
    Row {
        column: &'static str,
        #[source]
        source: sqlx::Error,
    },
    #[error("no runner matches the operator-supplied id")]
    RunnerNotFound,
    #[error("the lease cursor is outside the filtered runner history")]
    CursorInvalid,
}

impl Error {
    fn new(kind: ErrorKind) -> Self {
        Self {
            inner: Box::new(Inner {
                kind,
                backtrace: Backtrace::capture(),
            }),
        }
    }

    #[must_use]
    /// Registry code exposed to the API shell.
    pub const fn code(&self) -> ErrorCode {
        match self.inner.kind {
            ErrorKind::Datastore { .. } => error_code::INTERNAL_DB_UNAVAILABLE,
            ErrorKind::Query { .. } | ErrorKind::Row { .. } => error_code::INTERNAL_DB_QUERY,
            ErrorKind::RunnerNotFound => error_code::RUNNER_NOT_FOUND,
            ErrorKind::CursorInvalid => error_code::INVALID_REQUEST,
        }
    }

    #[must_use]
    /// Client-safe detail exposed to the API shell.
    pub const fn detail(&self) -> &'static str {
        match self.inner.kind {
            ErrorKind::Datastore { .. } => "Database unavailable",
            ErrorKind::Query { .. } | ErrorKind::Row { .. } => "Database error",
            ErrorKind::RunnerNotFound => "Runner not found",
            ErrorKind::CursorInvalid => super::runner_leases::DETAIL_BAD_CURSOR,
        }
    }

    #[must_use]
    /// Whether the backing datastore could not be reached at all.
    pub const fn is_datastore_unavailable(&self) -> bool {
        matches!(self.inner.kind, ErrorKind::Datastore { .. })
    }
}

impl From<afd_db::Error> for Error {
    fn from(source: afd_db::Error) -> Self {
        Self::new(ErrorKind::Datastore { source })
    }
}

pub(super) fn query(context: &'static str) -> impl Fn(sqlx::Error) -> Error {
    move |source| Error::new(ErrorKind::Query { context, source })
}

pub(super) fn row(column: &'static str) -> impl Fn(sqlx::Error) -> Error {
    move |source| Error::new(ErrorKind::Row { column, source })
}

pub(super) fn runner_not_found() -> Error {
    Error::new(ErrorKind::RunnerNotFound)
}

pub(super) fn cursor_invalid() -> Error {
    Error::new(ErrorKind::CursorInvalid)
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
        std::error::Error::source(&self.inner.kind)
    }
}

#[cfg(test)]
#[path = "error/tests.rs"]
mod tests;
