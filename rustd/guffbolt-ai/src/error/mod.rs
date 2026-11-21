//! What a wire fails with: one error type, written by hand because this crate
//! stands alone, with its [`Result`] beside it.
//!
//! No failure carries an endpoint, a key, a prompt or a provider's message:
//! a refusal names its status and nothing the provider wrote.

use std::fmt;

use rig_core::ProviderError;
use rig_core::message::EmptyToolName;

/// What a fallible wire call answers.
pub type Result<T, E = Error> = core::result::Result<T, E>;

/// A wire call that failed, with the typed failure that caused it as its
/// source.
pub struct Error(Box<ErrorKind>);

/// Every way a wire call fails.
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
        source: ProviderError,
    },
    #[error("inference endpoint is not a valid URL")]
    Url {
        #[source]
        source: url::ParseError,
    },
    #[error("inference endpoint names no host")]
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
    #[error("the input count belongs to a different prepared request or route")]
    CountStale,
    #[error("the provider count endpoint does not support this prepared input")]
    CountUnsupported,
    #[error("the provider input count is missing or invalid")]
    CountReply,
    #[error("the provider input count cannot be decoded")]
    CountDocument {
        #[source]
        source: serde_json::Error,
    },
    #[error("the provider input count failed with HTTP status {status}")]
    CountStatus { status: u16 },
    #[error("the provider input count transport failed")]
    CountTransport {
        #[source]
        source: rig_core::http_client::Error,
    },
    #[error("the count request line does not parse once its path is replaced")]
    KeyPath {
        #[source]
        source: http::uri::InvalidUri,
    },
    #[error("the count request URI does not reassemble once its path is replaced")]
    KeyUri {
        #[source]
        source: http::uri::InvalidUriParts,
    },
    /// The provider, or the transport under it, failed the turn.
    #[error("{}", Failed(.source))]
    Provider {
        #[source]
        source: ProviderError,
    },
    /// The conversation holds a result that answers no call before it.
    #[error("the conversation cannot be sent: the result for call {call_id} answers no call")]
    Unsendable { call_id: String },
    /// The conversation holds a call with no tool name.
    #[error("the conversation cannot be sent: call {call_id} names no tool")]
    Unnamed {
        call_id: String,
        #[source]
        source: EmptyToolName,
    },
    /// A turn's record was not the JSON its wire defines.
    #[error("the model provider's turn could not be written or read")]
    Unreadable {
        #[source]
        source: serde_json::Error,
    },
}

/// A provider failure in words: its status when it answered with one.
struct Failed<'a>(&'a ProviderError);

impl fmt::Display for Failed<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match status(self.0) {
            Some(status) => write!(
                f,
                "the model provider refused the turn with status {status}"
            ),
            None => f.write_str("the model provider failed the turn"),
        }
    }
}

/// The status a provider refused with, when it answered one.
fn status(failure: &ProviderError) -> Option<u16> {
    failure
        .provider_response_status()
        .filter(|status| !status.is_success())
        .map(|status| status.as_u16())
}

impl Error {
    pub(crate) fn kind(&self) -> &ErrorKind {
        &self.0
    }

    /// The provider, or the transport under it, failed the turn.
    pub(crate) fn provider(source: ProviderError) -> Self {
        Self::from(ErrorKind::Provider { source })
    }

    /// The result for `call_id` answers no call before it.
    pub(crate) fn unsendable(call_id: &str) -> Self {
        Self::from(ErrorKind::Unsendable {
            call_id: call_id.to_owned(),
        })
    }

    /// Call `call_id` names no tool, for `source`.
    pub(crate) fn unnamed(call_id: &str, source: EmptyToolName) -> Self {
        Self::from(ErrorKind::Unnamed {
            call_id: call_id.to_owned(),
            source,
        })
    }

    /// A turn's record that will not encode or decode, for `source`.
    pub(crate) fn unreadable(source: serde_json::Error) -> Self {
        Self::from(ErrorKind::Unreadable { source })
    }

    /// The HTTP status the provider refused the turn with, when it answered
    /// one other than success.
    #[must_use]
    pub fn status(&self) -> Option<u16> {
        match self.kind() {
            ErrorKind::Provider { source } => status(source),
            _ => None,
        }
    }

    /// Whether the reply passed the byte bound the embedder set
    /// ([`Connected::with_reply_bound`](crate::wire::Connected::with_reply_bound)).
    #[must_use]
    pub fn is_reply_too_large(&self) -> bool {
        matches!(self.kind(), ErrorKind::ReplyTooLarge) || self.caused_by(is_bound)
    }

    /// Whether the embedder's transport refused the destination
    /// ([`SendError::Refused`](crate::wire::transport::SendError::Refused)),
    /// which no retry or reopening repeats.
    #[must_use]
    pub fn is_refused(&self) -> bool {
        self.caused_by(is_refusal)
    }

    /// Whether the conversation itself cannot be sent: a call names no tool,
    /// a result answers no call, or a replay is not one this crate wrote.
    #[must_use]
    pub fn is_unsendable(&self) -> bool {
        matches!(
            self.kind(),
            ErrorKind::Unsendable { .. } | ErrorKind::Unnamed { .. } | ErrorKind::Unreadable { .. }
        )
    }

    /// Whether the request or the route is unusable as given: a caller's
    /// error, decided before or without the provider.
    #[must_use]
    pub fn is_invalid_request(&self) -> bool {
        matches!(
            self.kind(),
            ErrorKind::Document { .. }
                | ErrorKind::Encode { .. }
                | ErrorKind::Request { .. }
                | ErrorKind::Url { .. }
                | ErrorKind::Endpoint
                | ErrorKind::Dialect
                | ErrorKind::Body
                | ErrorKind::OutputLimit
                | ErrorKind::Multipart
                | ErrorKind::ContextLimit
                | ErrorKind::CountStale
                | ErrorKind::CountUnsupported
                | ErrorKind::KeyPath { .. }
                | ErrorKind::KeyUri { .. }
        )
    }

    /// The wire library's own failure, for an embedder that classifies it
    /// further; `Err` gives back any other failure unchanged.
    ///
    /// # Errors
    /// The failure is not the provider's or its transport's.
    pub fn into_provider_failure(self) -> Result<ProviderError, Self> {
        match *self.0 {
            ErrorKind::Provider { source } => Ok(source),
            kind => Err(Self(Box::new(kind))),
        }
    }

    /// The wire library's own failure, when the provider or its transport
    /// failed the turn.
    #[must_use]
    pub fn provider_failure(&self) -> Option<&ProviderError> {
        match self.kind() {
            ErrorKind::Provider { source } => Some(source),
            _ => None,
        }
    }

    /// Whether any failure beneath this one answers `matches`.
    fn caused_by(&self, matches: fn(&(dyn std::error::Error + 'static)) -> bool) -> bool {
        match self.kind() {
            ErrorKind::Provider { source } => walks(source, matches),
            kind => std::iter::successors(std::error::Error::source(kind), |&error| error.source())
                .any(matches),
        }
    }
}

/// Whether `failure` is this crate's byte bound.
fn is_bound(failure: &(dyn std::error::Error + 'static)) -> bool {
    failure
        .downcast_ref::<Error>()
        .is_some_and(|error| matches!(error.kind(), ErrorKind::ReplyTooLarge))
}

/// Whether `failure` is the embedder's guard refusing a destination.
fn is_refusal(failure: &(dyn std::error::Error + 'static)) -> bool {
    failure
        .downcast_ref::<crate::wire::transport::SendError>()
        .is_some_and(crate::wire::transport::SendError::is_refused)
}

/// Whether a transport failure beneath `failure` answers `matches`. rig keeps
/// a transport's failure behind its own and names no source for it, so the
/// walk starts there.
fn walks(failure: &ProviderError, matches: fn(&(dyn std::error::Error + 'static)) -> bool) -> bool {
    let ProviderError::Http(transport) = failure else {
        return false;
    };
    let first: &(dyn std::error::Error + 'static) = &**transport;
    std::iter::successors(Some(first), |&error| error.source()).any(matches)
}

/// Whether the transport ended `failure`'s read at the embedder's byte bound.
pub(crate) fn oversize(failure: &ProviderError) -> bool {
    walks(failure, is_bound)
}

/// Whether the embedder's guard refused `failure`'s destination.
pub(crate) fn refused(failure: &ProviderError) -> bool {
    walks(failure, is_refusal)
}

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

#[cfg(test)]
mod tests;
