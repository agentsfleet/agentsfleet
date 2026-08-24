//! The one error type this crate returns, and the wire code each failure maps to.
//!
//! Same shape as `afd_crypto::Error` and `afd_db::Error`: a struct with a
//! private kind and `is_*` accessors, boxed because the largest variant carries
//! a `redis::RedisError` and this type is the `Err` of `Result`s the request
//! path returns.
//!
//! # A missing consumer group is not an outage
//!
//! Redis answers a read against a vanished group with `NOGROUP`, and that is
//! recoverable in one step — recreate the group and read again. Folding it into
//! a generic command failure would lose that, which is why
//! [`Error::is_group_missing`] exists: the repair path in [`crate::streams`]
//! asks exactly this question, and nothing else has to guess.

use std::backtrace::Backtrace;
use std::fmt::{self, Display, Formatter};

use afd_core::error_code::{self, ErrorCode};

/// A Redis operation failed, or the configuration for one was malformed.
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
    MissingRedisUrl { knob: &'static str },

    #[error("{knob} must be a redis:// or rediss:// URL")]
    InvalidRedisUrl { knob: &'static str },

    #[error("the TLS certificate authority file {path} could not be read")]
    CaCertUnreadable {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("the {role} Redis is unreachable")]
    Unreachable {
        role: &'static str,
        #[source]
        source: Box<redis::RedisError>,
    },

    #[error("{command} did not answer within {waited_ms}ms")]
    Timeout {
        command: &'static str,
        waited_ms: u128,
    },

    #[error("{command} failed")]
    Command {
        command: &'static str,
        #[source]
        source: Box<redis::RedisError>,
    },

    #[error("the consumer group on {stream} does not exist")]
    GroupMissing { stream: String },

    #[error("the consumer group on {stream} already exists")]
    GroupExists { stream: String },

    #[error("a {what} reply was not the shape this client expects")]
    UnexpectedReply { what: &'static str },

    #[error("the subscription hub's connection is gone")]
    HubClosed,
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

    /// Whether a role's URL or certificate path was absent or malformed.
    #[must_use]
    pub fn is_config(&self) -> bool {
        matches!(
            self.inner.kind,
            ErrorKind::MissingRedisUrl { .. }
                | ErrorKind::InvalidRedisUrl { .. }
                | ErrorKind::CaCertUnreadable { .. }
        )
    }

    /// Whether Redis could not be reached, or did not answer in time.
    #[must_use]
    pub fn is_unavailable(&self) -> bool {
        matches!(
            self.inner.kind,
            ErrorKind::Unreachable { .. } | ErrorKind::Timeout { .. }
        )
    }

    /// Whether a command reached Redis and Redis refused it.
    #[must_use]
    pub fn is_command(&self) -> bool {
        matches!(
            self.inner.kind,
            ErrorKind::Command { .. } | ErrorKind::UnexpectedReply { .. }
        )
    }

    /// Whether the stream's consumer group has gone missing.
    ///
    /// Recoverable in one step, and the only failure here that is. See the
    /// module documentation.
    #[must_use]
    pub fn is_group_missing(&self) -> bool {
        matches!(self.inner.kind, ErrorKind::GroupMissing { .. })
    }

    /// Whether the consumer group is already there.
    ///
    /// The expected answer to an idempotent create, not a failure — which is
    /// why it is a question and not a substring of an error message. Reading it
    /// off `Display` was the first attempt, and it did not work: `Display`
    /// renders the kind, and the Redis code lives on the source.
    #[must_use]
    pub fn is_group_exists(&self) -> bool {
        matches!(self.inner.kind, ErrorKind::GroupExists { .. })
    }

    /// Whether the hub was shut down while a subscriber was waiting.
    #[must_use]
    pub fn is_hub_closed(&self) -> bool {
        matches!(self.inner.kind, ErrorKind::HubClosed)
    }

    /// The registry code a handler would surface for this failure.
    #[must_use]
    pub fn code(&self) -> ErrorCode {
        match self.inner.kind {
            ErrorKind::Command { .. }
            | ErrorKind::UnexpectedReply { .. }
            | ErrorKind::GroupMissing { .. }
            | ErrorKind::GroupExists { .. } => error_code::INTERNAL_OPERATION_FAILED,
            _ => error_code::STARTUP_REDIS_CONNECT,
        }
    }

    /// The backtrace captured when this error was constructed.
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

/// Classifies a failed command.
///
/// `NOGROUP` is pulled out by name because it is the one recoverable failure:
/// the group vanished (deleted out of band, a restart without persistence, a
/// failover to an empty replica) and recreating it is a defined repair. Redis
/// reports it as an ordinary error reply, so nothing else would tell it apart
/// from a genuine command failure.
pub(crate) fn classify(command: &'static str, stream: &str, source: redis::RedisError) -> Error {
    match source.code() {
        Some("NOGROUP") => {
            return Error::new(ErrorKind::GroupMissing {
                stream: stream.to_owned(),
            });
        }
        Some("BUSYGROUP") => {
            return Error::new(ErrorKind::GroupExists {
                stream: stream.to_owned(),
            });
        }
        _ => {}
    }
    if source.is_connection_dropped() || source.is_io_error() {
        return Error::new(ErrorKind::Unreachable {
            role: "default",
            source: Box::new(source),
        });
    }
    Error::new(ErrorKind::Command {
        command,
        source: Box::new(source),
    })
}

/// A command that never answered inside its deadline.
pub(crate) fn timed_out(command: &'static str, waited_ms: u128) -> Error {
    Error::new(ErrorKind::Timeout { command, waited_ms })
}

/// A reply whose shape the client does not recognise.
pub(crate) fn unexpected_reply(what: &'static str) -> Error {
    Error::new(ErrorKind::UnexpectedReply { what })
}

/// One error of every kind, for tests that walk the whole surface.
///
/// Same seam and same argument as `afd_db::error::one_of_each_kind`: these are
/// the renderings a human reads while something is already wrong, and a Redis
/// that refuses a command on demand is not something a test can arrange for
/// every kind.
#[cfg(feature = "test-util")]
#[must_use]
pub fn one_of_each_kind() -> Vec<(&'static str, Error)> {
    let redis_failure = || {
        redis::RedisError::from((
            redis::ErrorKind::Extension,
            "refused",
            "the server said no".to_owned(),
        ))
    };

    vec![
        (
            "missing url",
            Error::new(ErrorKind::MissingRedisUrl { knob: "REDIS_URL" }),
        ),
        (
            "invalid url",
            Error::new(ErrorKind::InvalidRedisUrl { knob: "REDIS_URL" }),
        ),
        (
            "ca cert unreadable",
            Error::new(ErrorKind::CaCertUnreadable {
                path: "/tls/ca.crt".to_owned(),
                source: std::io::Error::from(std::io::ErrorKind::NotFound),
            }),
        ),
        (
            "unreachable",
            Error::new(ErrorKind::Unreachable {
                role: "default",
                source: Box::new(redis_failure()),
            }),
        ),
        ("timeout", timed_out("XADD", 5_000)),
        (
            "command",
            classify("XADD", "fleet:x:events", redis_failure()),
        ),
        (
            "group missing",
            Error::new(ErrorKind::GroupMissing {
                stream: "fleet:x:events".to_owned(),
            }),
        ),
        (
            "group exists",
            Error::new(ErrorKind::GroupExists {
                stream: "fleet:x:events".to_owned(),
            }),
        ),
        ("unexpected reply", unexpected_reply("PING")),
        ("hub closed", Error::new(ErrorKind::HubClosed)),
    ]
}
