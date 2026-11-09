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
    #[error("NATS_URL is unset; set it to the broker's comma-separated seed URLs.")]
    NatsUrlUnset,
    #[error("NATS_URL holds a seed that is not a NATS URL.")]
    NatsUrl {
        #[source]
        source: std::io::Error,
    },
    #[error("Every NATS_URL seed must carry the same user and a non-empty password.")]
    NatsCredentials,
    #[error("AGENTSFLEET_DEV_NAMESPACE must be at most 64 letters, digits, '-' or '_'.")]
    NatsNamespace {
        #[source]
        source: garde::Report,
    },
    #[error("A NATS adapter limit is outside its supported range.")]
    NatsTuning {
        #[source]
        source: garde::Report,
    },
    #[error("No NATS_URL seed answered before the connect deadline.")]
    NatsConnect {
        #[source]
        source: async_nats::ConnectError,
    },
    #[error("The NATS server at NATS_URL is unsuitable: {field}.")]
    NatsUnsuitable {
        /// The server-reported property that failed the boot check.
        field: &'static str,
    },
    #[error("The NATS connection is down; the signal is owed a gap marker.")]
    NatsDisconnected,
    #[error("The NATS client could not publish the signal.")]
    NatsPublish {
        #[source]
        source: async_nats::PublishError,
    },
    #[error("The NATS client could not subscribe.")]
    NatsSubscribe {
        #[source]
        source: async_nats::SubscribeError,
    },
    #[error("The NATS server did not confirm the subscription.")]
    NatsFlush {
        #[source]
        source: async_nats::client::FlushError,
    },
    #[error("The NATS connection did not drain.")]
    NatsDrain {
        #[source]
        source: async_nats::client::DrainError,
    },
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
    async_nats::ConnectError => NatsConnect,
    async_nats::PublishError => NatsPublish,
    async_nats::SubscribeError => NatsSubscribe,
    async_nats::client::FlushError => NatsFlush,
    async_nats::client::DrainError => NatsDrain,
);

impl Error {
    /// Finite diagnostic category; native messages and payloads never become labels.
    #[must_use]
    pub const fn reason(&self) -> &'static str {
        match self.kind() {
            ErrorKind::Dragonfly { .. }
            | ErrorKind::NatsConnect { .. }
            | ErrorKind::NatsUnsuitable { .. }
            | ErrorKind::NatsDisconnected
            | ErrorKind::NatsPublish { .. }
            | ErrorKind::NatsSubscribe { .. }
            | ErrorKind::NatsFlush { .. }
            | ErrorKind::NatsDrain { .. } => "backend",
            ErrorKind::Json { .. } => "serialization",
            ErrorKind::Closed => "closed",
            ErrorKind::Timeout { .. } => "timeout",
            ErrorKind::Bounds { .. }
            | ErrorKind::Capacity { .. }
            | ErrorKind::NatsUrlUnset
            | ErrorKind::NatsUrl { .. }
            | ErrorKind::NatsCredentials
            | ErrorKind::NatsNamespace { .. }
            | ErrorKind::NatsTuning { .. } => "configuration",
        }
    }

    /// Existing registry code; no broker implementation enters the public vocabulary.
    #[must_use]
    pub fn code(&self) -> ErrorCode {
        match self.kind() {
            ErrorKind::Dragonfly { source } => source.code(),
            ErrorKind::NatsUrlUnset
            | ErrorKind::NatsUrl { .. }
            | ErrorKind::NatsCredentials
            | ErrorKind::NatsNamespace { .. }
            | ErrorKind::NatsTuning { .. } => error_code::STARTUP_ENV_CHECK,
            ErrorKind::NatsConnect { .. } | ErrorKind::NatsUnsuitable { .. } => {
                error_code::STARTUP_DRAGONFLY_CONNECT
            }
            ErrorKind::Json { .. }
            | ErrorKind::Closed
            | ErrorKind::Timeout { .. }
            | ErrorKind::Bounds { .. }
            | ErrorKind::Capacity { .. }
            | ErrorKind::NatsDisconnected
            | ErrorKind::NatsPublish { .. }
            | ErrorKind::NatsSubscribe { .. }
            | ErrorKind::NatsFlush { .. }
            | ErrorKind::NatsDrain { .. } => error_code::INTERNAL_OPERATION_FAILED,
        }
    }

    /// Safe sentence with the native diagnostic retained in the source chain.
    #[must_use]
    pub const fn detail(&self) -> &'static str {
        af_core::error::DETAIL_OPERATION_FAILED
    }
}
