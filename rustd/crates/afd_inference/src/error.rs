//! Configuration errors carry no endpoint or credential bytes.

/// Result of validating a persisted inference configuration.
pub type Result<T, E = Error> = core::result::Result<T, E>;

afd_core::error_shell!(
    /// A model's persisted inference settings are unusable.
    pub struct Error(ErrorKind);
);

#[derive(Debug, thiserror::Error)]
pub(crate) enum ErrorKind {
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
    #[error("model output limit exceeds its context window")]
    OutputLimit,
    #[error("complete inference input exhausts the model context window")]
    ContextLimit,
}

afd_core::error_lifts!(Error, ErrorKind:
    rig_core::error::EncodeError => Encode,
    rig_core::ProviderError => Request,
    garde::Report => Bounds,
    url::ParseError => Url,
);

impl Error {
    /// Invalid configuration is a caller error, independent of provider availability.
    #[must_use]
    pub const fn code(&self) -> afd_core::error_code::ErrorCode {
        afd_core::error_code::INVALID_REQUEST
    }
}
