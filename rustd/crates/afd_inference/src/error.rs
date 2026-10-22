//! Configuration errors carry no endpoint or credential bytes.

/// Result of validating a persisted inference configuration.
pub type Result<T, E = Error> = core::result::Result<T, E>;

afd_core::error_shell!(
    /// A model's persisted inference settings are unusable.
    pub struct Error(ErrorKind);
);

#[derive(Debug, thiserror::Error)]
pub(crate) enum ErrorKind {
    #[error("the provider document cannot be decoded")]
    Document {
        #[source]
        source: serde_json::Error,
    },
    #[error("inference request cannot be encoded")]
    Encode {
        #[source]
        source: rig_core::error::EncodeError,
    },
    #[error("inference request is unsupported")]
    Request {
        #[source]
        source: rig_core::ProviderError,
    },
    #[error("inference settings exceed their declared bounds")]
    Bounds {
        #[source]
        source: garde::Report,
    },
    #[error("inference endpoint is not a valid URL")]
    Url {
        #[source]
        source: url::ParseError,
    },
    #[error("inference endpoint must be HTTPS without userinfo, query or fragment")]
    Endpoint,
    #[error("inference dialect is not supported for the configured API")]
    Dialect,
    #[error("inference request must contain JSON bytes")]
    Body,
    #[error("model output limit exceeds its context window")]
    OutputLimit,
    #[error("the model provider response exceeds its byte bound")]
    ReplyTooLarge,
    #[error("the model provider response deadline expired")]
    StreamDeadline,
    #[error("the model turn was cancelled")]
    StreamCancelled,
    #[error("the model transport does not upload multipart bodies")]
    Multipart,
    #[error("complete inference input exhausts the model context window")]
    ContextLimit,
    #[error("the model API key is not a valid header value")]
    KeyHeader {
        #[source]
        source: http::header::InvalidHeaderValue,
    },
    #[error("the Gemini request line does not parse once its key is lifted out")]
    KeyPath {
        #[source]
        source: http::uri::InvalidUri,
    },
    #[error("the Gemini request URI does not reassemble once its key is lifted out")]
    KeyUri {
        #[source]
        source: http::uri::InvalidUriParts,
    },
}

afd_core::error_lifts!(Error, ErrorKind:
    serde_json::Error => Document,
    rig_core::error::EncodeError => Encode,
    rig_core::ProviderError => Request,
    garde::Report => Bounds,
    url::ParseError => Url,
    http::header::InvalidHeaderValue => KeyHeader,
    http::uri::InvalidUri => KeyPath,
    http::uri::InvalidUriParts => KeyUri,
);

impl Error {
    /// Whether the shared transport stopped an oversized response.
    #[must_use]
    pub fn is_oversize(&self) -> bool {
        matches!(self.kind(), ErrorKind::ReplyTooLarge)
    }

    /// Invalid configuration is a caller error, independent of provider availability.
    #[must_use]
    pub fn code(&self) -> afd_core::error_code::ErrorCode {
        match self.kind() {
            ErrorKind::ReplyTooLarge | ErrorKind::StreamDeadline | ErrorKind::StreamCancelled => {
                afd_core::error_code::INTERNAL_OPERATION_FAILED
            }
            _ => afd_core::error_code::INVALID_REQUEST,
        }
    }
}

impl From<Error> for rig_core::ProviderError {
    fn from(source: Error) -> Self {
        Self::from_transport_error(rig_core::http_client::Error::instance(source))
    }
}
