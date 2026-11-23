//! Why an environment call failed, sorted the way a tool reads it.

use std::fmt;
use std::sync::Arc;

/// The log code a failure carries when its environment names none.
const ENV_FAILED: &str = "env_failed";

/// Which kind of failure a tool must tell apart from the rest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvErrorKind {
    /// The path left the root, by name or through a link.
    PathRefused,
    /// A file call named something the root does not hold.
    NotFound,
    /// A process call named a process the environment no longer holds: one
    /// that ended, or never was.
    UnknownProcess,
    /// The process would not take what was written to it: it closed its
    /// input, or has not read what it was sent. It runs on.
    InputRefused,
    /// Anything else: the environment could not be reached or would not do
    /// it.
    Unavailable,
}

/// What a fallible environment call answers.
pub(crate) type Result<T, E = EnvError> = core::result::Result<T, E>;

/// A failure an environment call carries as its cause.
type Cause = Arc<dyn std::error::Error + Send + Sync>;

/// An environment call that failed: its kind, what the model reads, what a
/// log line records, and the failure that caused it, when one did.
#[derive(Debug, Clone)]
pub struct EnvError {
    kind: EnvErrorKind,
    message: String,
    log_code: &'static str,
    log_reason: String,
    source: Option<Cause>,
}

impl EnvError {
    /// A failure of `kind`, which the model reads as `message`; its log line
    /// records the same words.
    #[must_use]
    pub fn new(kind: EnvErrorKind, message: impl Into<String>) -> Self {
        let message = message.into();
        Self {
            kind,
            log_reason: message.clone(),
            message,
            log_code: ENV_FAILED,
            source: None,
        }
    }

    /// The same failure, caused by `source`, which its `source()` answers.
    #[must_use]
    pub fn caused_by(mut self, source: impl std::error::Error + Send + Sync + 'static) -> Self {
        self.source = Some(Arc::new(source));
        self
    }

    /// The same failure, logged under the embedder's `code` with `reason`,
    /// which may say more than the model is told.
    #[must_use]
    pub fn logged_as(mut self, code: &'static str, reason: impl Into<String>) -> Self {
        self.log_code = code;
        self.log_reason = reason.into();
        self
    }

    /// Which kind it is.
    #[must_use]
    pub const fn kind(&self) -> EnvErrorKind {
        self.kind
    }

    /// What the model reads.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// The code its log line records.
    #[must_use]
    pub const fn log_code(&self) -> &'static str {
        self.log_code
    }

    /// The reason its log line records.
    #[must_use]
    pub fn log_reason(&self) -> &str {
        &self.log_reason
    }

    /// Whether the path left the root.
    #[must_use]
    pub fn is_path_refused(&self) -> bool {
        self.kind == EnvErrorKind::PathRefused
    }

    /// Whether the call named something the root does not hold.
    #[must_use]
    pub fn is_not_found(&self) -> bool {
        self.kind == EnvErrorKind::NotFound
    }

    /// Whether the call named a process the environment no longer holds.
    #[must_use]
    pub fn is_unknown_process(&self) -> bool {
        self.kind == EnvErrorKind::UnknownProcess
    }

    /// Whether the process would not take what was written to it.
    #[must_use]
    pub fn is_input_refused(&self) -> bool {
        self.kind == EnvErrorKind::InputRefused
    }
}

impl fmt::Display for EnvError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for EnvError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source
            .as_deref()
            .map(|source| source as &(dyn std::error::Error + 'static))
    }
}
