//! HTTP refusals and retry decisions share one finite error classification.

use super::{Error, ErrorKind};
use af_core::error_code::{self, ErrorCode};
use afd_runner::capacity::authority::Refusal;
use std::borrow::Cow;

impl Error {
    /// Render through the common problem catalogue; never choose status ad hoc.
    #[must_use]
    pub fn response(&self) -> axum::response::Response {
        use axum::response::IntoResponse as _;
        if let ErrorKind::Capacity { carrying, capacity } = self.kind() {
            return afd_http::handler::Refusal::at_stream_ceiling(*carrying, *capacity)
                .into_response();
        }
        afd_http::envelope::ProblemResponse::new(
            self.code(),
            self.detail().into_owned(),
            afd_http::request_id::RequestId::mint(),
        )
        .into_response()
    }

    /// Registered classification used by agents and HTTP problem responses.
    #[must_use]
    pub fn code(&self) -> ErrorCode {
        match self.kind() {
            ErrorKind::Authority { refusal } => refusal.code(),
            ErrorKind::UnsupportedVersion => error_code::RUN_PROTOCOL_UNSUPPORTED,
            ErrorKind::Identifier { .. } | ErrorKind::SessionIdentifier { .. } => {
                error_code::UUIDV7_INVALID_ID_SHAPE
            }
            ErrorKind::MissingHeader { .. }
            | ErrorKind::RepeatedHeader { .. }
            | ErrorKind::HeaderEncoding { .. }
            | ErrorKind::UnknownStream
            | ErrorKind::ControlBearerMissing
            | ErrorKind::UnexpectedBearer
            | ErrorKind::Protocol
            | ErrorKind::Conversion { .. }
            | ErrorKind::DuplicateStream { .. }
            | ErrorKind::Header { .. }
            | ErrorKind::Bounds { .. }
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
            ErrorKind::Closed
            | ErrorKind::ChannelSend { .. }
            | ErrorKind::Completion { .. }
            | ErrorKind::Timeout { .. }
            | ErrorKind::Io { .. }
            | ErrorKind::Worker { .. }
            | ErrorKind::Http { .. } => error_code::INTERNAL_OPERATION_FAILED,
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
            ErrorKind::Protocol | ErrorKind::Conversion { .. } => "The message is not valid for this stream or session state.".into(),
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
            ErrorKind::Closed | ErrorKind::ChannelSend { .. } | ErrorKind::Completion { .. } | ErrorKind::Transport { .. } | ErrorKind::Io { .. } => "The connection ended. Reconnect before the last confirmed lease deadline.".into(),
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
            ErrorKind::Authority { .. }
            | ErrorKind::Protocol
            | ErrorKind::Conversion { .. }
            | ErrorKind::UnsupportedVersion
            | ErrorKind::MissingHeader { .. }
            | ErrorKind::RepeatedHeader { .. }
            | ErrorKind::HeaderEncoding { .. }
            | ErrorKind::UnknownStream
            | ErrorKind::ControlBearerMissing
            | ErrorKind::UnexpectedBearer
            | ErrorKind::SessionIdentifier { .. }
            | ErrorKind::Binding
            | ErrorKind::DuplicateStream { .. }
            | ErrorKind::Header { .. }
            | ErrorKind::Identifier { .. }
            | ErrorKind::Bounds { .. }
            | ErrorKind::Json { .. } => false,
            ErrorKind::Transport { source } => matches!(
                source.reason(),
                af_stream::Reason::Network
                    | af_stream::Reason::Timeout
                    | af_stream::Reason::Closed
                    | af_stream::Reason::Worker
            ),
            ErrorKind::Fleet { .. } | ErrorKind::Authoring { .. } | ErrorKind::Runner { .. } => {
                matches!(
                    af_core::problem::Problem::of(self.code()).status(),
                    408 | 429 | 500..=599
                )
            }
            ErrorKind::Capacity { .. }
            | ErrorKind::Closed
            | ErrorKind::ChannelSend { .. }
            | ErrorKind::Completion { .. }
            | ErrorKind::Timeout { .. }
            | ErrorKind::Io { .. }
            | ErrorKind::Database { .. }
            | ErrorKind::Query { .. }
            | ErrorKind::Bus { .. }
            | ErrorKind::Worker { .. }
            | ErrorKind::Http { .. } => true,
        }
    }

    /// Finite observation category; callers never need to match an error sentence.
    #[must_use]
    pub fn reason(&self) -> &'static str {
        match self.kind() {
            ErrorKind::UnsupportedVersion => "protocol_version",
            ErrorKind::MissingHeader { .. }
            | ErrorKind::RepeatedHeader { .. }
            | ErrorKind::HeaderEncoding { .. }
            | ErrorKind::UnknownStream
            | ErrorKind::ControlBearerMissing
            | ErrorKind::UnexpectedBearer
            | ErrorKind::SessionIdentifier { .. }
            | ErrorKind::Header { .. } => "metadata",
            ErrorKind::Protocol
            | ErrorKind::Conversion { .. }
            | ErrorKind::Identifier { .. }
            | ErrorKind::Json { .. } => "protocol",
            ErrorKind::Binding | ErrorKind::DuplicateStream { .. } => "binding",
            ErrorKind::Authority {
                refusal: Refusal::Credential,
            } => "credential",
            ErrorKind::Authority {
                refusal: Refusal::Revoked,
            } => "revoked",
            ErrorKind::Authority {
                refusal: Refusal::Superseded,
            } => "superseded",
            ErrorKind::Bounds { .. } => "bounds",
            ErrorKind::Transport { source } => source.reason().as_str(),
            ErrorKind::Timeout { .. } => "timeout",
            ErrorKind::Closed | ErrorKind::ChannelSend { .. } | ErrorKind::Completion { .. } => {
                "closed"
            }
            ErrorKind::Io { .. } => "network",
            ErrorKind::Worker { .. } => "worker",
            ErrorKind::Database { .. } | ErrorKind::Query { .. } => "database",
            ErrorKind::Bus { .. } => "broker",
            ErrorKind::Capacity { .. } => "capacity",
            ErrorKind::Runner { .. } | ErrorKind::Fleet { .. } | ErrorKind::Authoring { .. } => {
                "service"
            }
            ErrorKind::Http { .. } => "internal",
        }
    }
}
