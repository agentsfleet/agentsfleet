//! One fixed daemon-owned transport; the seam is the physical provider request.

mod network;
mod prompt;
mod request;
mod resolver;
mod response;

use std::time::Duration;

use crate::Result;
use afd_crypto::secret::SecretString;

pub use network::Network;

/// The Responses API keys and item kinds this adapter both writes and reads.
///
/// One spelling for the request builder, the tool schema and the decoder, so an
/// item the daemon sends is recognised by the same name when it comes back.
const KEY_TYPE: &str = "type";
/// See [`KEY_TYPE`].
const KEY_CONTENT: &str = "content";
/// See [`KEY_TYPE`].
const KEY_CALL_ID: &str = "call_id";
/// See [`KEY_TYPE`].
const KIND_FUNCTION_CALL: &str = "function_call";
/// The single argument the proposal tool takes: the proposal as one JSON text.
const PROPOSAL_ARGUMENT: &str = "proposal_json";
pub(super) use prompt::{SYSTEM, tool};
pub(super) use request::body;
pub(super) use response::{Decoded, decode};

/// A request assembled by the daemon after authorization and durable intent.
pub struct ProviderRequest {
    pub(super) key: SecretString,
    pub(super) body: Vec<u8>,
    pub(super) timeout: Duration,
}

impl ProviderRequest {
    /// Fixed provider JSON; contains private context and must never be logged.
    #[must_use]
    pub fn body(&self) -> &[u8] {
        &self.body
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
    /// At most the protocol envelope ceiling.
    pub body: Vec<u8>,
}

impl std::fmt::Debug for ProviderResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderResponse")
            .field("status", &self.status)
            .finish_non_exhaustive()
    }
}

/// Sends once. SDK retries, redirects and destination overrides are forbidden.
#[async_trait::async_trait]
pub trait Transport: Send + Sync + std::fmt::Debug {
    /// Performs one physical request and returns its bounded response.
    ///
    /// # Errors
    /// No authoritative response arrived; accounting must remain unknown.
    async fn send(&self, request: ProviderRequest) -> Result<ProviderResponse>;
}
