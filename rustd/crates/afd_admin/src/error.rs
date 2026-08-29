//! The one error type returned by platform administration repositories.

use std::backtrace::Backtrace;
use std::fmt::{self, Display, Formatter};

use afd_core::error_code::{self, ErrorCode};

/// Result returned by every fallible administration operation.
pub type Result<T, E = Error> = core::result::Result<T, E>;

/// A platform administration operation failed.
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
    #[error("the datastore backing platform administration would not answer")]
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
    #[error("a {table} row holds an unreadable {column}")]
    Row {
        table: &'static str,
        column: &'static str,
        #[source]
        source: afd_core::error::Error,
    },
    #[error("could not draw entropy for a model identifier")]
    Entropy {
        #[source]
        source: afd_crypto::error::Error,
    },
    #[error("a model identifier could not represent the current instant")]
    Mint {
        #[source]
        source: afd_core::error::Error,
    },
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

    /// Registry code exposed to the API shell.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        match self.inner.kind {
            ErrorKind::Datastore { .. } => error_code::INTERNAL_DB_UNAVAILABLE,
            ErrorKind::Query { .. } | ErrorKind::Row { .. } => error_code::INTERNAL_DB_QUERY,
            ErrorKind::Entropy { .. } | ErrorKind::Mint { .. } => {
                error_code::INTERNAL_OPERATION_FAILED
            }
        }
    }

    /// Client-safe detail exposed to the API shell.
    #[must_use]
    pub const fn detail(&self) -> &'static str {
        match self.inner.kind {
            ErrorKind::Datastore { .. } => "Database unavailable",
            ErrorKind::Query { .. } | ErrorKind::Row { .. } => "Database error",
            ErrorKind::Entropy { .. } | ErrorKind::Mint { .. } => "Internal operation failed",
        }
    }

    /// Whether the backing datastore could not be reached at all.
    #[must_use]
    pub const fn is_datastore_unavailable(&self) -> bool {
        matches!(self.inner.kind, ErrorKind::Datastore { .. })
    }
}

impl From<afd_db::Error> for Error {
    fn from(source: afd_db::Error) -> Self {
        Self::new(ErrorKind::Datastore { source })
    }
}

impl From<afd_crypto::error::Error> for Error {
    fn from(source: afd_crypto::error::Error) -> Self {
        Self::new(ErrorKind::Entropy { source })
    }
}

impl From<afd_core::error::Error> for Error {
    fn from(source: afd_core::error::Error) -> Self {
        Self::new(ErrorKind::Mint { source })
    }
}

pub(super) fn query(context: &'static str) -> impl Fn(sqlx::Error) -> Error {
    move |source| Error::new(ErrorKind::Query { context, source })
}

pub(super) fn row(
    table: &'static str,
    column: &'static str,
) -> impl Fn(afd_core::error::Error) -> Error {
    move |source| {
        Error::new(ErrorKind::Row {
            table,
            column,
            source,
        })
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
        std::error::Error::source(&self.inner.kind)
    }
}

#[cfg(test)]
#[path = "error/tests.rs"]
mod tests;
