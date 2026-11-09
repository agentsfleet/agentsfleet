//! Finite failure classes with their original causes retained.

use af_core::error_code::{self, ErrorCode};

/// This crate's fallible result, including explicitly named foreign errors.
pub type Result<T, E = Error> = core::result::Result<T, E>;

mod labels;

pub use labels::{Reason, Stage};

af_core::error_shell!(
    /// A stream failure with its source and captured backtrace.
    pub struct Error(ErrorKind);
);

#[derive(Debug, thiserror::Error)]
pub(crate) enum ErrorKind {
    #[error("The outbound proxy refused CONNECT with HTTP status {status}.")]
    ProxyRefused { status: http::StatusCode },
    #[error("The {setting} proxy setting is unsupported.")]
    ProxyPolicy { setting: &'static str },
    #[error("The {setting} proxy setting could not be parsed.")]
    ProxyInput {
        setting: &'static str,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    #[error("Set {setting} to a whole number of mebibytes between 12 and 512.")]
    MemorySetting {
        setting: &'static str,
        #[source]
        source: std::num::ParseIntError,
    },
    #[error("The runner message does not match the stream protocol.")]
    Protocol,
    #[error("The message body could not be read.")]
    Body {
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    #[error("The runner channel requires X25519MLKEM768; the peer selected {negotiated:?}.")]
    KeyExchangeRefused { negotiated: crate::tls::KeyExchange },
    #[error("The TLS certificates or trust settings could not be loaded.")]
    Tls {
        #[source]
        source: boring::error::ErrorStack,
    },
    #[error("The secure connection could not be established.")]
    TlsHandshake {
        #[source]
        source: tokio_boring::HandshakeError<tokio::net::TcpStream>,
    },
    #[error("HTTP/2 failed")]
    H2 {
        #[source]
        source: h2::Error,
    },
    #[error("The HTTP connection could not complete the exchange.")]
    Hyper {
        #[source]
        source: hyper::Error,
    },
    #[error("HTTP message was refused")]
    Http {
        #[source]
        source: http::Error,
    },
    #[error("An HTTP header name is invalid.")]
    HeaderName {
        #[source]
        source: http::header::InvalidHeaderName,
    },
    #[error("An HTTP header value is invalid.")]
    HeaderValue {
        #[source]
        source: http::header::InvalidHeaderValue,
    },
    #[error("The HTTP method is invalid.")]
    Method {
        #[source]
        source: http::method::InvalidMethod,
    },
    #[error("The HTTP response status is invalid.")]
    Status {
        #[source]
        source: http::status::InvalidStatusCode,
    },
    #[error("HTTP URI was refused")]
    Uri {
        #[source]
        source: http::uri::InvalidUri,
    },
    #[error("endpoint URL was refused")]
    Url {
        #[source]
        source: url::ParseError,
    },
    #[error("connection entropy failed")]
    Entropy {
        #[source]
        source: af_crypto::error::Error,
    },
    #[error("QUIC operation failed")]
    Quic {
        #[source]
        source: tokio_quiche::quiche::Error,
    },
    #[error("QUIC handshake failed")]
    QuicHandshake {
        #[source]
        source: std::io::Error,
    },
    #[error("HTTP/3 failed")]
    H3 {
        #[source]
        source: tokio_quiche::quiche::h3::Error,
    },
    #[error("HTTP/3 driver failed")]
    H3Driver {
        #[source]
        source: tokio_quiche::http3::driver::H3ConnectionError,
    },
    #[error("QUIC listener failed")]
    QuicListener {
        #[source]
        source: std::io::Error,
    },
    #[error("HTTP URI parts were refused")]
    UriParts {
        #[source]
        source: http::uri::InvalidUriParts,
    },
    #[error("stream reply owner disappeared")]
    Reply {
        #[source]
        source: tokio::sync::oneshot::error::RecvError,
    },
    #[error("The outgoing stream is closed.")]
    Send {
        #[source]
        source: tokio_util::sync::PollSendError<tokio_quiche::http3::driver::OutboundFrame>,
    },
    #[error("The certificate and key file paths must use UTF-8.")]
    IdentityPath,
    #[error("The connection operation was cancelled.")]
    Cancelled,
    #[error("Timed out while {stage}.")]
    Timeout {
        stage: Stage,
        #[source]
        source: tokio::time::error::Elapsed,
    },
    #[error("The connection closed while {stage}.")]
    Closed { stage: Stage },
    #[error("The connection could not read or write data.")]
    Io {
        #[source]
        source: std::io::Error,
    },
    #[error("The message size is outside the allowed range.")]
    Bounds {
        #[source]
        source: garde::Report,
    },
    #[error("The runner message has an invalid JSON structure.")]
    Json {
        #[source]
        source: serde_json::Error,
    },
    #[error("frame length cannot be represented")]
    Length {
        #[source]
        source: std::num::TryFromIntError,
    },
    #[error("frame memory budget closed")]
    Budget {
        #[source]
        source: tokio::sync::AcquireError,
    },
    #[error("A connection worker stopped unexpectedly.")]
    Worker {
        #[source]
        source: tokio::task::JoinError,
    },
}

af_core::error_lifts!(Error, ErrorKind:
    boring::error::ErrorStack => Tls,
    tokio_boring::HandshakeError<tokio::net::TcpStream> => TlsHandshake,
    h2::Error => H2,
    hyper::Error => Hyper,
    http::Error => Http,
    http::header::InvalidHeaderName => HeaderName,
    http::header::InvalidHeaderValue => HeaderValue,
    http::method::InvalidMethod => Method,
    http::status::InvalidStatusCode => Status,
    http::uri::InvalidUri => Uri,
    url::ParseError => Url,
    af_crypto::error::Error => Entropy,
    tokio_quiche::quiche::Error => Quic,
    tokio_quiche::quiche::h3::Error => H3,
    tokio_quiche::http3::driver::H3ConnectionError => H3Driver,
    http::uri::InvalidUriParts => UriParts,
    tokio::sync::oneshot::error::RecvError => Reply,
    tokio_util::sync::PollSendError<tokio_quiche::http3::driver::OutboundFrame> => Send,
    std::io::Error => Io,
    garde::Report => Bounds,
    serde_json::Error => Json,
    std::num::TryFromIntError => Length,
    tokio::sync::AcquireError => Budget,
    tokio::task::JoinError => Worker,
);

impl Error {
    /// Safe network detail without rendering a body, credential or URI.
    #[must_use]
    pub fn io_kind(&self) -> Option<std::io::ErrorKind> {
        match self.kind() {
            ErrorKind::Io { source } | ErrorKind::QuicHandshake { source } => Some(source.kind()),
            ErrorKind::TlsHandshake { source } => source.as_io_error().map(std::io::Error::kind),
            ErrorKind::H2 { source } => source.get_io().map(std::io::Error::kind),
            _ => None,
        }
    }

    /// Registry code shared with other failed internal operations.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        match self.kind() {
            ErrorKind::KeyExchangeRefused { .. } => error_code::RUN_PROTOCOL_UNSUPPORTED,
            _ => error_code::INTERNAL_OPERATION_FAILED,
        }
    }

    /// Stable sentence; the source chain retains diagnostic detail.
    #[must_use]
    pub const fn detail(&self) -> &'static str {
        af_core::error::DETAIL_OPERATION_FAILED
    }

    /// Finite category suitable for recovery decisions and metric labels.
    #[must_use]
    pub fn reason(&self) -> Reason {
        match self.kind() {
            ErrorKind::MemorySetting { .. }
            | ErrorKind::ProxyPolicy { .. }
            | ErrorKind::ProxyInput { .. } => Reason::Protocol,
            ErrorKind::ProxyRefused { status } => {
                if status.is_server_error() || *status == http::StatusCode::TOO_MANY_REQUESTS {
                    Reason::Network
                } else {
                    Reason::Protocol
                }
            }
            ErrorKind::Io { .. } => Reason::Network,
            ErrorKind::Timeout { .. } => Reason::Timeout,
            ErrorKind::Cancelled => Reason::Cancelled,
            ErrorKind::Closed { .. }
            | ErrorKind::Budget { .. }
            | ErrorKind::Reply { .. }
            | ErrorKind::Send { .. } => Reason::Closed,
            ErrorKind::Worker { .. } => Reason::Worker,
            ErrorKind::Tls { .. }
            | ErrorKind::IdentityPath
            | ErrorKind::KeyExchangeRefused { .. } => Reason::Tls,
            ErrorKind::TlsHandshake { source } => {
                if source.as_io_error().is_some() {
                    Reason::Network
                } else {
                    Reason::Tls
                }
            }
            ErrorKind::H2 { source } => {
                if source.is_io() {
                    Reason::Network
                } else {
                    Reason::Protocol
                }
            }
            ErrorKind::Hyper { source } => {
                if source.is_parse() || source.is_user() {
                    Reason::Protocol
                } else {
                    Reason::Network
                }
            }
            ErrorKind::Quic { source } => quic_reason(*source),
            ErrorKind::QuicHandshake { source } => match source.get_ref() {
                Some(cause) if cause.is::<tokio_quiche::quic::HandshakeError>() => Reason::Network,
                Some(cause) => cause
                    .downcast_ref::<tokio_quiche::quiche::Error>()
                    .map_or(Reason::Tls, |cause| quic_reason(*cause)),
                None => Reason::Network,
            },
            ErrorKind::Body { .. } | ErrorKind::Entropy { .. } | ErrorKind::QuicListener { .. } => {
                Reason::Worker
            }
            ErrorKind::Protocol
            | ErrorKind::Bounds { .. }
            | ErrorKind::Json { .. }
            | ErrorKind::Length { .. }
            | ErrorKind::Http { .. }
            | ErrorKind::Uri { .. }
            | ErrorKind::Url { .. }
            | ErrorKind::UriParts { .. }
            | ErrorKind::H3 { .. }
            | ErrorKind::H3Driver { .. }
            | ErrorKind::HeaderName { .. }
            | ErrorKind::HeaderValue { .. }
            | ErrorKind::Method { .. }
            | ErrorKind::Status { .. } => Reason::Protocol,
        }
    }
}

fn quic_reason(source: tokio_quiche::quiche::Error) -> Reason {
    use tokio_quiche::quiche::Error as Quic;
    match source {
        Quic::TlsFail | Quic::CryptoFail => Reason::Tls,
        Quic::Done => Reason::Network,
        Quic::UnknownVersion => Reason::Unsupported,
        _ => Reason::Protocol,
    }
}

pub(crate) async fn within<T>(
    stop: &tokio_util::sync::CancellationToken,
    deadline: tokio::time::Instant,
    stage: Stage,
    future: impl Future<Output = Result<T>>,
) -> Result<T> {
    use tracing::Instrument as _;
    let observation = crate::telemetry::Observation::operation(stage);
    let result = async {
        tokio::select! {
            biased;
            () = stop.cancelled() => Err(ErrorKind::Cancelled.into()),
            result = tokio::time::timeout_at(deadline, future) => {
                match result {
                    Ok(result) => result,
                    Err(source) => Err(ErrorKind::Timeout { stage, source }.into()),
                }
            }
        }
    }
    .instrument(observation.span())
    .await;
    observation.finish(result)
}
