//! Configuration errors carry no endpoint or credential bytes.

/// Result of validating a persisted inference configuration.
pub type Result<T, E = Error> = core::result::Result<T, E>;

af_core::error_shell!(
    /// A model's persisted inference settings are unusable.
    pub struct Error(ErrorKind);
);

#[derive(Debug, thiserror::Error)]
pub(crate) enum ErrorKind {
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
    #[error("active decision settings require Noul and Choice capabilities")]
    DecisionCapabilities,
    /// The route the settings name is unusable: its words are the route's.
    #[error(transparent)]
    Route { source: guffbolt_ai::Error },
}

af_core::error_lifts!(Error, ErrorKind:
    garde::Report => Bounds,
    url::ParseError => Url,
    guffbolt_ai::Error => Route,
);

impl Error {
    /// Invalid configuration is a caller error, independent of provider availability.
    #[must_use]
    pub const fn code(&self) -> af_core::error_code::ErrorCode {
        match self.kind() {
            ErrorKind::Bounds { .. }
            | ErrorKind::Url { .. }
            | ErrorKind::Endpoint
            | ErrorKind::DecisionCapabilities
            | ErrorKind::Route { .. } => af_core::error_code::INVALID_REQUEST,
        }
    }
}
