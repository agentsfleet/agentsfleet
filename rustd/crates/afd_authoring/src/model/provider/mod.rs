//! Daemon-owned credentials and a single physical send through the selected model API.

mod dispatch;
mod edits;
mod network;
mod prompt;
mod rejections;
mod request;
mod resolver;
mod response;
mod schema;

#[cfg(test)]
mod tests;

use std::time::Duration;

use crate::Result;
pub(in crate::model) use dispatch::{Outbound, complete};
pub(in crate::model) use rejections::validate_key;

pub use network::Network;

/// The Responses API keys and item kinds this adapter both writes and reads.
///
/// One spelling for the request builder, the tool schema and the decoder, so an
/// item the daemon sends is recognised by the same name when it comes back.
const KEY_TYPE: &str = "type";
/// The JSON Schema keyword describing a tool, field, or policy object.
const KEY_DESCRIPTION: &str = "description";
/// The single argument carries the native proposal object without JSON-in-JSON.
const PROPOSAL_ARGUMENT: &str = "proposal";
pub(super) use prompt::system;
pub(super) use request::body;
pub(super) use response::{Decoded, decode};

/// A request assembled by the daemon after authorization and durable intent.
pub struct ProviderRequest {
    pub(super) request: http::Request<Vec<u8>>,
    pub(super) timeout: Duration,
}

impl ProviderRequest {
    /// SDK-encoded provider JSON; contains private context and must never be logged.
    #[must_use]
    pub fn body(&self) -> &[u8] {
        self.request.body()
    }
}

impl std::fmt::Debug for ProviderRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderRequest")
            .field("timeout", &self.timeout)
            .finish_non_exhaustive()
    }
}

/// A bounded response at the physical provider boundary.
pub struct ProviderResponse {
    /// Provider HTTP status; response bodies are never error detail.
    pub status: u16,
    /// The real provider headers; the SDK checks the declared stream framing.
    pub headers: http::HeaderMap,
    /// Incremental bytes; the shared inference adapter enforces the wire bound.
    pub body: rig_core::http_client::BoxedStream,
}

impl std::fmt::Debug for ProviderResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderResponse")
            .field("status", &self.status)
            .finish_non_exhaustive()
    }
}

/// Sends once. Retries and redirects are forbidden; the destination comes from the persisted model.
#[async_trait::async_trait]
pub trait Transport: Send + Sync + std::fmt::Debug {
    /// Performs one physical request and returns its response before collecting the body.
    ///
    /// # Errors
    /// No authoritative response arrived; accounting must remain unknown.
    async fn send(&self, request: ProviderRequest) -> Result<ProviderResponse>;
}
