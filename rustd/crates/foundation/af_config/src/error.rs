//! Configuration faults preserve typed causes and safe field locations.

use std::path::PathBuf;

use af_core::error_code::{self, ErrorCode};

/// Configuration loading and validation result.
pub type Result<T> = std::result::Result<T, Error>;

af_core::error_shell!(
    /// A settings file, invocation layer or typed validation could not be used.
    pub struct Error(ErrorKind);
);

#[derive(Debug, thiserror::Error)]
pub(crate) enum ErrorKind {
    #[error("cannot read configuration {}", path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("configuration {} requires valid TOML at line {line}", path.display())]
    Syntax {
        path: PathBuf,
        line: usize,
        #[source]
        source: toml::de::Error,
    },
    #[error("configuration {} has an invalid setting at {field}", path.display())]
    Field {
        path: PathBuf,
        field: String,
        #[source]
        source: toml::de::Error,
    },
    #[error("configuration {} failed validation ({source})", path.display())]
    Validation {
        path: PathBuf,
        #[source]
        source: garde::Report,
    },
}

impl Error {
    /// Uses the existing startup configuration refusal code.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        error_code::STARTUP_ENV_CHECK
    }
}
