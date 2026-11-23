//! A provider's failed turns, as a wire reports them, for an embedder's suites
//! that name a failure without a provider to fail.

use rig_core::ProviderError;
use rig_core::http_client as rig_http;

use crate::Error;

/// The provider answered `status`, with `body`.
#[must_use]
pub fn answered(status: http::StatusCode, body: &str) -> Error {
    Error::provider(ProviderError::from_http_response(status, body))
}

/// The reply was cut short.
#[must_use]
pub fn cut() -> Error {
    Error::provider(ProviderError::Truncated)
}

/// The provider ended its reply with its own error, `body`.
#[must_use]
pub fn ended(body: &str) -> Error {
    Error::provider(ProviderError::from_provider_body(body))
}

/// The provider failed with `diagnostic` and kept no reply.
#[must_use]
pub fn diagnosed(diagnostic: &str) -> Error {
    Error::provider(ProviderError::Provider(diagnostic.to_owned()))
}

/// The reply decoded but does not answer the request, for `reason`.
#[must_use]
pub fn unanswered(reason: &str) -> Error {
    Error::provider(ProviderError::Response(reason.to_owned()))
}

/// The connection failed with `source` before the provider replied.
#[must_use]
pub fn dropped(source: std::io::Error) -> Error {
    Error::provider(ProviderError::from_transport_error(
        rig_http::Error::transient(source),
    ))
}
