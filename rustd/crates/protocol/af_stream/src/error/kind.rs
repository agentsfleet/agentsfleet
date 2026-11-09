//! Transport error cases retain their original typed causes.

use super::Stage;

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
    #[error("The key exchange observation owner disappeared.")]
    KeyExchangeObservation {
        #[source]
        source: tokio::sync::watch::error::RecvError,
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
    #[error("The retained frame exceeds its available memory allowance.")]
    Retention {
        #[source]
        source: tokio::sync::TryAcquireError,
    },
    #[error("A connection worker stopped unexpectedly.")]
    Worker {
        #[source]
        source: tokio::task::JoinError,
    },
}
