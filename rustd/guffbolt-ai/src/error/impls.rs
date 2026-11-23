//! The traits an [`Error`] answers to, and the failures it is lifted from.

use std::fmt;

use rig_core::ProviderError;

use super::{Error, ErrorKind};

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl std::error::Error for Error {
    // The kind is this error's own text, so its source is the kind's.
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        std::error::Error::source(&*self.0)
    }
}

impl From<ErrorKind> for Error {
    fn from(kind: ErrorKind) -> Self {
        Self(Box::new(kind))
    }
}

impl From<serde_json::Error> for Error {
    fn from(source: serde_json::Error) -> Self {
        Self::from(ErrorKind::Document { source })
    }
}

impl From<rig_core::error::EncodeError> for Error {
    fn from(source: rig_core::error::EncodeError) -> Self {
        Self::from(ErrorKind::Encode { source })
    }
}

impl From<ProviderError> for Error {
    fn from(source: ProviderError) -> Self {
        Self::from(ErrorKind::Request { source })
    }
}

impl From<url::ParseError> for Error {
    fn from(source: url::ParseError) -> Self {
        Self::from(ErrorKind::Url { source })
    }
}

impl From<http::uri::InvalidUri> for Error {
    fn from(source: http::uri::InvalidUri) -> Self {
        Self::from(ErrorKind::KeyPath { source })
    }
}

impl From<http::uri::InvalidUriParts> for Error {
    fn from(source: http::uri::InvalidUriParts) -> Self {
        Self::from(ErrorKind::KeyUri { source })
    }
}

impl From<rig_core::http_client::Error> for Error {
    fn from(source: rig_core::http_client::Error) -> Self {
        Self::from(ErrorKind::CountTransport { source })
    }
}

impl From<Error> for ProviderError {
    fn from(source: Error) -> Self {
        // Retryable, as a transport's own failure was before rig split
        // transient from permanent.
        Self::from_transport_error(rig_core::http_client::Error::transient(source))
    }
}
