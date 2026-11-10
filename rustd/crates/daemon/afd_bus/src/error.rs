//! Bus failures retain native causes behind the shared crate error shape.

use af_core::error_code::{self, ErrorCode};

/// Fallible bus operations preserve native causes.
pub type Result<T, E = Error> = core::result::Result<T, E>;

af_core::error_shell!(
    /// Signal failure with its native cause and captured backtrace.
    pub struct Error(ErrorKind);
);

#[derive(Debug, thiserror::Error)]
pub(crate) enum ErrorKind {
    #[error("The work-notification capacity is outside its supported bounds.")]
    Bounds {
        #[source]
        source: garde::Report,
    },
    #[error("The work-notification capacity must be a positive whole number.")]
    Capacity {
        #[source]
        source: std::num::ParseIntError,
    },
    #[error("The message broker could not complete the operation.")]
    Dragonfly {
        #[source]
        source: afd_dragonfly::Error,
    },
    #[error("The live notification could not be encoded as JSON.")]
    Json {
        #[source]
        source: serde_json::Error,
    },
    #[error("The message subscription or broker has closed.")]
    Closed,
    #[error("The message broker did not answer before the deadline.")]
    Timeout {
        #[source]
        source: tokio::time::error::Elapsed,
    },
}

af_core::error_lifts!(Error, ErrorKind:
    garde::Report => Bounds,
    std::num::ParseIntError => Capacity,
    afd_dragonfly::Error => Dragonfly,
    serde_json::Error => Json,
    tokio::time::error::Elapsed => Timeout,
);

impl Error {
    /// Finite diagnostic category; native messages and payloads never become labels.
    #[must_use]
    pub const fn reason(&self) -> &'static str {
        match self.kind() {
            ErrorKind::Dragonfly { .. } => "backend",
            ErrorKind::Json { .. } => "serialization",
            ErrorKind::Closed => "closed",
            ErrorKind::Timeout { .. } => "timeout",
            ErrorKind::Bounds { .. } | ErrorKind::Capacity { .. } => "configuration",
        }
    }

    /// Existing registry code; no broker implementation enters the public vocabulary.
    #[must_use]
    pub fn code(&self) -> ErrorCode {
        match self.kind() {
            ErrorKind::Dragonfly { source } => source.code(),
            ErrorKind::Json { .. }
            | ErrorKind::Closed
            | ErrorKind::Timeout { .. }
            | ErrorKind::Bounds { .. }
            | ErrorKind::Capacity { .. } => error_code::INTERNAL_OPERATION_FAILED,
        }
    }

    /// Safe sentence with the native diagnostic retained in the source chain.
    #[must_use]
    pub const fn detail(&self) -> &'static str {
        af_core::error::DETAIL_OPERATION_FAILED
    }
}
