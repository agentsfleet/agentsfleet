//! Typed session failures retain their service and transport causes.

use crate::Operation;
use afd_runner::capacity::authority::Refusal;

#[derive(Debug, thiserror::Error)]
pub(crate) enum ErrorKind {
    #[error("The message is not valid for this runner stream or session state.")]
    Protocol,
    #[error("The runner stream value could not be represented.")]
    Conversion {
        #[source]
        source: std::num::TryFromIntError,
    },
    #[error("Required runner stream header '{header}' is missing.")]
    MissingHeader { header: &'static str },
    #[error("Send runner stream header '{header}' exactly once.")]
    RepeatedHeader { header: &'static str },
    #[error("Runner stream header '{header}' must contain valid text.")]
    HeaderEncoding {
        header: &'static str,
        #[source]
        source: http::header::ToStrError,
    },
    #[error("Runner and daemon use different stream protocols. Update them to matching versions.")]
    UnsupportedVersion,
    #[error("Choose a runner stream kind: control, work, reports or activity.")]
    UnknownStream,
    #[error("Authenticate the control kind before attaching the remaining streams.")]
    ControlBearerMissing,
    #[error("Send the bearer only on control; secondary streams use the authenticated session.")]
    UnexpectedBearer,
    #[error("The runner session header must contain a valid UUIDv7 identifier.")]
    SessionIdentifier {
        #[source]
        source: af_core::error::Error,
    },
    #[error("This stream does not belong to the authenticated connection.")]
    Binding,
    #[error("The {kind:?} kind is already attached to this connection.")]
    DuplicateStream { kind: af_wire::protocol::StreamKind },
    #[error("This daemon has no available runner stream slots. Retry later.")]
    Capacity { carrying: usize, capacity: usize },
    #[error("The runner stream closed before the operation completed.")]
    Closed,
    #[error("The runner stream channel closed before delivery.")]
    ChannelSend {
        #[source]
        source: tokio::sync::mpsc::error::SendError<()>,
    },
    #[error("The runner stream completion owner disappeared.")]
    Completion {
        #[source]
        source: tokio::sync::oneshot::error::RecvError,
    },
    #[error("The runner stream no longer has permission to serve this host.")]
    Authority { refusal: Refusal },
    #[error("The runner stream could not read an HTTP header.")]
    Header {
        #[source]
        source: http::header::ToStrError,
    },
    #[error("The runner stream contains an invalid identifier.")]
    Identifier {
        #[source]
        source: af_core::error::Error,
    },
    #[error("The runner stream message is outside its supported bounds.")]
    Bounds {
        #[source]
        source: garde::Report,
    },
    #[error("The runner stream message could not be read as JSON.")]
    Json {
        #[source]
        source: serde_json::Error,
    },
    #[error("The runner stream transport could not complete the operation.")]
    Transport {
        #[source]
        source: af_stream::Error,
    },
    #[error("The runner stream timed out while {operation}.")]
    Timeout {
        operation: Operation,
        #[source]
        source: tokio::time::error::Elapsed,
    },
    #[error("The runner stream could not read or write its body.")]
    Io {
        #[source]
        source: std::io::Error,
    },
    #[error("The runner stream could not read its durable state.")]
    Database {
        #[source]
        source: afd_db::Error,
    },
    #[error("The notification broker is unavailable. Database recovery remains active.")]
    Bus {
        #[source]
        source: afd_bus::Error,
    },
    #[error("The runner stream database statement could not complete.")]
    Query {
        #[source]
        source: sqlx::Error,
    },
    #[error("The runner service could not complete the operation.")]
    Runner {
        #[source]
        source: afd_runner::Error,
    },
    #[error("The fleet service could not complete the operation.")]
    Fleet {
        #[source]
        source: afd_fleet::Error,
    },
    #[error("The authoring service could not complete the operation.")]
    Authoring {
        #[source]
        source: afd_authoring::Error,
    },
    #[error("The runner stream worker stopped before completing its work.")]
    Worker {
        #[source]
        source: tokio::task::JoinError,
    },
    #[error("The runner stream could not construct its HTTP response.")]
    Http {
        #[source]
        source: http::Error,
    },
}
