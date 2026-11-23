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

/// How the provider, or the transport under it, failed a turn
/// ([`Error::failure`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Failure {
    /// The provider answered with a status other than success.
    Refused {
        /// The HTTP status.
        status: u16,
    },
    /// The connection dropped, or the reply was cut short, before the turn
    /// ended.
    Lost,
    /// The provider ended the turn with an error of its own.
    Ended,
    /// The wire could not build the request, or read the reply as its wire
    /// defines it.
    Unread,
}

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
    /// No provider of that id was added to the collection.
    #[cfg(feature = "reqwest")]
    #[error("provider {provider} is not registered")]
    Unregistered { provider: Box<str> },
    /// The provider's catalogue holds no row of that id.
    #[cfg(feature = "reqwest")]
    #[error("provider {provider} has no model {model}")]
    UnknownModel { provider: Box<str>, model: Box<str> },
    /// The variable the provider's key is read from is unset or empty.
    #[cfg(feature = "reqwest")]
    #[error("the provider's key is missing: set {variable}")]
    MissingKey { variable: &'static str },
    /// A compiled-in catalogue file is not the rows it should hold.
    #[cfg(feature = "reqwest")]
    #[error("the model catalogue of provider {provider} cannot be read")]
    Catalog {
        provider: Box<str>,
        #[source]
        source: serde_json::Error,
    },
    /// The default transport's client could not be built.
    #[cfg(feature = "reqwest")]
    #[error("the default transport cannot be built")]
    Client {
        #[source]
        source: reqwest::Error,
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
    /// error, decided before or without the provider. A provider not
    /// registered, a model its catalogue lacks and a key not set are three.
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
        ) || self.is_unfound()
    }

    /// Whether a lookup in [`Models`](crate::Models) found no provider, no
    /// model or no key.
    #[cfg(feature = "reqwest")]
    const fn is_unfound(&self) -> bool {
        matches!(
            *self.0,
            ErrorKind::Unregistered { .. }
                | ErrorKind::UnknownModel { .. }
                | ErrorKind::MissingKey { .. }
        )
    }

    /// Without a `Models` collection, no lookup fails.
    #[cfg(not(feature = "reqwest"))]
    #[expect(
        clippy::unused_self,
        reason = "the same call as the lookup build's, which reads the kind"
    )]
    const fn is_unfound(&self) -> bool {
        false
    }

    /// How the provider, or the transport under it, failed the turn; `None`
    /// for a failure that is neither's.
    #[must_use]
    pub fn failure(&self) -> Option<Failure> {
        let ErrorKind::Provider { source } = self.kind() else {
            return None;
        };
        if let Some(status) = status(source) {
            return Some(Failure::Refused { status });
        }
        Some(match source {
            ProviderError::Http(_) | ProviderError::Truncated => Failure::Lost,
            ProviderError::Provider(_) => Failure::Ended,
            _ if source.provider_response().is_some() => Failure::Ended,
            _ => Failure::Unread,
        })
    }

    /// The provider's own name for the failure, such as
    /// `context_length_exceeded`, when its reply named one. It is the
    /// provider's text: an embedder that reports it checks it reads as a name.
    #[must_use]
    pub fn provider_code(&self) -> Option<String> {
        match self.kind() {
            ErrorKind::Provider { source } => source
                .provider_response()
                .and_then(rig_core::ProviderResponseError::machine_code),
            _ => None,
        }
    }

    /// The provider's or the transport's own failure beneath this one, for an
    /// embedder that keeps it as the cause of its own error; `Err` gives back
    /// any failure [`Error::failure`] names no kind of, unchanged.
    ///
    /// # Errors
    /// The failure is not the provider's or its transport's.
    pub fn into_cause(self) -> Result<Box<dyn std::error::Error + Send + Sync>, Self> {
        match *self.0 {
            ErrorKind::Provider { source } => Ok(Box::new(source)),
            kind => Err(Self(Box::new(kind))),
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

mod impls;

#[cfg(test)]
mod tests;
