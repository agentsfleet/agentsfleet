//! Session failures preserve domain and transport causes behind the shared error shell.

use af_core::error_code::{self, ErrorCode};
use afd_runner::capacity::authority::Refusal;
use std::borrow::Cow;

use crate::Operation;

/// Fallible session operations retain the service that refused them.
pub type Result<T, E = Error> = core::result::Result<T, E>;

af_core::error_shell!(
    /// A safe session failure with its original cause and captured backtrace.
    pub struct Error(ErrorKind);
);

#[derive(Debug, thiserror::Error)]
pub(crate) enum ErrorKind {
    #[error("The message is not valid for this runner stream or session state.")]
    Protocol,
    #[error("Required runner stream header '{header}' is missing.")]
    MissingHeader { header: &'static str },
    #[error("Send runner stream header '{header}' exactly once.")]
    RepeatedHeader { header: &'static str },
    #[error("Runner stream header '{header}' must contain valid text.")]
    HeaderEncoding { header: &'static str, #[source] source: http::header::ToStrError },
    #[error("Runner and daemon use different stream protocols. Update them to matching versions.")]
    UnsupportedVersion,
    #[error("Choose a runner stream kind: control, work, reports or activity.")]
    UnknownStream,
    #[error("Authenticate the control kind before attaching the remaining streams.")]
    ControlBearerMissing,
    #[error("Send the bearer only on control; secondary streams use the authenticated session.")]
    UnexpectedBearer,
    #[error("The runner session header must contain a valid UUIDv7 identifier.")]
    SessionIdentifier { #[source] source: af_core::error::Error },
    #[error("This stream does not belong to the authenticated connection.")]
    Binding,
    #[error("The {kind:?} kind is already attached to this connection.")]
    DuplicateStream { kind: af_wire::protocol::StreamKind },
    #[error("This daemon has no available runner stream slots. Retry later.")]
    Capacity { carrying: usize, capacity: usize },
    #[error("The runner stream closed before the operation completed.")]
    Closed,
    #[error("The runner stream no longer has permission to serve this host.")]
    Authority { refusal: Refusal },
    #[error("The runner stream could not read an HTTP header.")]
    Header { #[source] source: http::header::ToStrError },
    #[error("The runner stream contains an invalid identifier.")]
    Identifier { #[source] source: af_core::error::Error },
    #[error("The runner stream message is outside its supported bounds.")]
    Bounds { #[source] source: garde::Report },
    #[error("The runner stream message could not be read as JSON.")]
    Json { #[source] source: serde_json::Error },
    #[error("The runner stream transport could not complete the operation.")]
    Transport { #[source] source: af_stream::Error },
    #[error("The runner stream timed out while {operation}.")]
    Timeout { operation: Operation, #[source] source: tokio::time::error::Elapsed },
    #[error("The runner stream could not read or write its body.")]
    Io { #[source] source: std::io::Error },
    #[error("The runner stream could not read its durable state.")]
    Database { #[source] source: afd_db::Error },
    #[error("The notification broker is unavailable. Database recovery remains active.")]
    Bus { #[source] source: afd_bus::Error },
    #[error("The runner stream database statement could not complete.")]
    Query { #[source] source: sqlx::Error },
    #[error("The runner service could not complete the operation.")]
    Runner { #[source] source: afd_runner::Error },
    #[error("The fleet service could not complete the operation.")]
    Fleet { #[source] source: afd_fleet::Error },
    #[error("The authoring service could not complete the operation.")]
    Authoring { #[source] source: afd_authoring::Error },
    #[error("The runner stream worker stopped before completing its work.")]
    Worker { #[source] source: tokio::task::JoinError },
    #[error("The runner stream could not construct its HTTP response.")]
    Http { #[source] source: http::Error },
}

af_core::error_lifts!(Error, ErrorKind:
    http::header::ToStrError => Header,
    af_core::error::Error => Identifier,
    garde::Report => Bounds,
    serde_json::Error => Json,
    af_stream::Error => Transport,
    std::io::Error => Io,
    afd_db::Error => Database,
    afd_bus::Error => Bus,
    sqlx::Error => Query,
    afd_runner::Error => Runner,
    afd_fleet::Error => Fleet,
    afd_authoring::Error => Authoring,
    tokio::task::JoinError => Worker,
    http::Error => Http,
);

impl Error {
    /// Render through the common problem catalogue; never choose status ad hoc.
    #[must_use]
    pub fn response(&self) -> axum::response::Response {
        use axum::response::IntoResponse as _;
        if let ErrorKind::Capacity { carrying, capacity } = self.kind() {
            return afd_http::handler::Refusal::at_stream_ceiling(*carrying, *capacity).into_response();
        }
        afd_http::envelope::ProblemResponse::new(self.code(), self.detail().into_owned(),
            afd_http::request_id::RequestId::mint()).into_response()
    }

    /// Registered classification used by agents and HTTP problem responses.
    #[must_use]
    pub fn code(&self) -> ErrorCode {
        match self.kind() {
            ErrorKind::Authority { refusal } => refusal.code(),
            ErrorKind::UnsupportedVersion => error_code::RUN_PROTOCOL_UNSUPPORTED,
            ErrorKind::Identifier { .. }
            | ErrorKind::SessionIdentifier { .. } => error_code::UUIDV7_INVALID_ID_SHAPE,
            ErrorKind::MissingHeader { .. } | ErrorKind::RepeatedHeader { .. }
            | ErrorKind::HeaderEncoding { .. } | ErrorKind::UnknownStream
            | ErrorKind::ControlBearerMissing | ErrorKind::UnexpectedBearer
            | ErrorKind::Protocol | ErrorKind::DuplicateStream { .. }
            | ErrorKind::Header { .. } | ErrorKind::Bounds { .. }
            | ErrorKind::Json { .. } => error_code::INVALID_REQUEST,
            ErrorKind::Binding => error_code::AUTH_FORBIDDEN,
            ErrorKind::Capacity { .. } => error_code::SSE_STREAM_CAP,
            ErrorKind::Fleet { source } => source.code(),
            ErrorKind::Authoring { source } => source.code(),
            ErrorKind::Runner { source } => source.code(),
            ErrorKind::Database { source } => source.code(),
            ErrorKind::Bus { source } => source.code(),
            ErrorKind::Query { .. } => error_code::INTERNAL_DB_QUERY,
            ErrorKind::Transport { source } => source.code(),
            ErrorKind::Closed | ErrorKind::Timeout { .. } | ErrorKind::Io { .. }
            | ErrorKind::Worker { .. } | ErrorKind::Http { .. } => error_code::INTERNAL_OPERATION_FAILED,
        }
    }

    /// Safe operator sentence; causes remain available through `source()`.
    #[must_use]
    pub fn detail(&self) -> Cow<'static, str> {
        match self.kind() {
            ErrorKind::MissingHeader { .. } | ErrorKind::RepeatedHeader { .. }
            | ErrorKind::HeaderEncoding { .. } | ErrorKind::UnsupportedVersion
            | ErrorKind::UnknownStream | ErrorKind::ControlBearerMissing
            | ErrorKind::UnexpectedBearer | ErrorKind::SessionIdentifier { .. } => self.kind().to_string().into(),
            ErrorKind::Protocol => "The message is not valid for this stream or session state.".into(),
            ErrorKind::Binding => "Attach this kind to the physical connection that authenticated its session.".into(),
            ErrorKind::DuplicateStream { kind } => format!("The {} kind is already attached. Open each kind once.", kind.as_str()).into(),
            ErrorKind::Header { .. } => "A runner stream header is not valid text.".into(),
            ErrorKind::Identifier { .. } => "The message contains an invalid UUIDv7 identifier.".into(),
            ErrorKind::Bounds { .. } => "The message exceeds the permitted size, entry count or field bounds.".into(),
            ErrorKind::Json { .. } => "The message is not valid JSON for this runner protocol.".into(),
            ErrorKind::Authority { refusal: Refusal::Credential } => "The runner credential is no longer valid. Update it before reconnecting.".into(),
            ErrorKind::Authority { refusal: Refusal::Revoked } => "This runner is no longer active. Check its administrative status.".into(),
            ErrorKind::Authority { refusal: Refusal::Superseded } => "Another connection owns this runner. Stop this process to avoid competing connections.".into(),
            ErrorKind::Capacity { .. } => "This daemon has no available stream slots. Retry after the advertised delay.".into(),
            ErrorKind::Timeout { operation, .. } => format!("The stream timed out while {operation}. Retain the last confirmed lease deadline.").into(),
            ErrorKind::Closed | ErrorKind::Transport { .. } | ErrorKind::Io { .. } => "The connection ended. Reconnect before the last confirmed lease deadline.".into(),
            ErrorKind::Database { .. } | ErrorKind::Query { .. } => "The database did not confirm the operation. Retain the last confirmed lease deadline.".into(),
            ErrorKind::Bus { .. } => "The notification broker is unavailable. Database recovery remains active.".into(),
            ErrorKind::Worker { .. } => "A session worker stopped. Reconnect before the last confirmed lease deadline.".into(),
            ErrorKind::Http { .. } => "The server could not construct its response. Retry with backoff.".into(),
            ErrorKind::Fleet { source } => source.detail().into(),
            ErrorKind::Runner { source } => source.detail().into(),
            ErrorKind::Authoring { source } => source.detail().into(),
        }
    }

    /// Whether reconnecting can help without changing credentials or protocol.
    #[must_use]
    pub fn retryable(&self) -> bool {
        match self.kind() {
            ErrorKind::Authority { .. } | ErrorKind::Protocol | ErrorKind::UnsupportedVersion
            | ErrorKind::MissingHeader { .. } | ErrorKind::RepeatedHeader { .. }
            | ErrorKind::HeaderEncoding { .. } | ErrorKind::UnknownStream
            | ErrorKind::ControlBearerMissing | ErrorKind::UnexpectedBearer
            | ErrorKind::SessionIdentifier { .. }
            | ErrorKind::Binding | ErrorKind::DuplicateStream { .. }
            | ErrorKind::Header { .. } | ErrorKind::Identifier { .. }
            | ErrorKind::Bounds { .. } | ErrorKind::Json { .. } => false,
            ErrorKind::Transport { source } => matches!(source.reason(), af_stream::Reason::Network
                | af_stream::Reason::Timeout | af_stream::Reason::Closed | af_stream::Reason::Worker),
            ErrorKind::Fleet { .. } | ErrorKind::Authoring { .. } | ErrorKind::Runner { .. } => {
                matches!(af_core::problem::Problem::of(self.code()).status(), 408 | 429 | 500..=599)
            }
            ErrorKind::Capacity { .. } | ErrorKind::Closed | ErrorKind::Timeout { .. }
            | ErrorKind::Io { .. } | ErrorKind::Database { .. } | ErrorKind::Query { .. }
            | ErrorKind::Bus { .. } | ErrorKind::Worker { .. } | ErrorKind::Http { .. } => true,
        }
    }

    /// Finite observation category; callers never need to match an error sentence.
    #[must_use]
    pub fn reason(&self) -> &'static str {
        match self.kind() {
            ErrorKind::UnsupportedVersion => "protocol_version",
            ErrorKind::MissingHeader { .. } | ErrorKind::RepeatedHeader { .. }
            | ErrorKind::HeaderEncoding { .. } | ErrorKind::UnknownStream
            | ErrorKind::ControlBearerMissing | ErrorKind::UnexpectedBearer
            | ErrorKind::SessionIdentifier { .. } | ErrorKind::Header { .. } => "metadata",
            ErrorKind::Protocol | ErrorKind::Identifier { .. } | ErrorKind::Json { .. } => "protocol",
            ErrorKind::Binding | ErrorKind::DuplicateStream { .. } => "binding",
            ErrorKind::Authority { refusal: Refusal::Credential } => "credential",
            ErrorKind::Authority { refusal: Refusal::Revoked } => "revoked",
            ErrorKind::Authority { refusal: Refusal::Superseded } => "superseded",
            ErrorKind::Bounds { .. } => "bounds",
            ErrorKind::Transport { source } => source.reason().as_str(),
            ErrorKind::Timeout { .. } => "timeout", ErrorKind::Closed => "closed",
            ErrorKind::Io { .. } => "network", ErrorKind::Worker { .. } => "worker",
            ErrorKind::Database { .. } | ErrorKind::Query { .. } => "database",
            ErrorKind::Bus { .. } => "broker",
            ErrorKind::Capacity { .. } => "capacity",
            ErrorKind::Runner { .. } | ErrorKind::Fleet { .. }
            | ErrorKind::Authoring { .. } => "service",
            ErrorKind::Http { .. } => "internal",
        }
    }
}

pub(crate) async fn within<T>(
    stop: &tokio_util::sync::CancellationToken,
    deadline: tokio::time::Instant,
    operation: Operation,
    future: impl Future<Output = Result<T>>,
) -> Result<T> {
    tokio::select! {
        biased;
        () = stop.cancelled() => Err(ErrorKind::Closed.into()),
        result = tokio::time::timeout_at(deadline, future) => {
            result.map_err(|source| ErrorKind::Timeout { operation, source })?
        }
    }
}
