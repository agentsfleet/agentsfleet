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

mod kind;
pub(crate) use kind::ErrorKind;

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
    tokio::sync::watch::error::RecvError => KeyExchangeObservation,
    tokio_util::sync::PollSendError<tokio_quiche::http3::driver::OutboundFrame> => Send,
    std::io::Error => Io,
    garde::Report => Bounds,
    serde_json::Error => Json,
    std::num::TryFromIntError => Length,
    tokio::sync::AcquireError => Budget,
    tokio::sync::TryAcquireError => Retention,
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
            ErrorKind::ProxyRefused { status } => proxy_reason(*status),
            ErrorKind::Io { .. } => Reason::Network,
            ErrorKind::Timeout { .. } => Reason::Timeout,
            ErrorKind::Cancelled => Reason::Cancelled,
            ErrorKind::Closed { .. }
            | ErrorKind::Budget { .. }
            | ErrorKind::Reply { .. }
            | ErrorKind::KeyExchangeObservation { .. }
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
            | ErrorKind::Origin
            | ErrorKind::Retention { .. }
            | ErrorKind::MemorySetting { .. }
            | ErrorKind::ProxyPolicy { .. }
            | ErrorKind::ProxyInput { .. }
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

#[cfg(test)]
mod tests;

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

fn proxy_reason(status: http::StatusCode) -> Reason {
    if status.is_server_error() || status == http::StatusCode::TOO_MANY_REQUESTS {
        Reason::Network
    } else {
        Reason::Protocol
    }
}
