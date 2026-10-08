//! What the three egress tools share: sending through the lease's guard, and
//! how a refusal and an answer read to the model.
//!
//! `http_request`, `web_fetch` and `pushover` differ only in the request they
//! draft and how they word the answer; admission, the transport, the mask and
//! the refusal's code and log line are the same for each, so they live here
//! once.

use std::borrow::Cow;
use std::sync::Arc;

use afr_egress::{Draft, Error, Inbound, Refusal, Transport};

use crate::catalog::Entry;
use crate::lease::Lease;
use crate::runtime::{ToolErrorCode, ToolOutput};

/// The event a refused call logs under, fixed because dashboards match on it.
const EVENT_TOOL_REFUSED: &str = "tool_refused";

/// The statuses a tool reports as succeeded.
const SUCCESS: std::ops::Range<u16> = 200..300;

/// The transport every egress tool sends through, shared across leases.
pub(crate) type SharedTransport = Arc<dyn Transport>;

impl From<Refusal> for ToolErrorCode {
    fn from(refusal: Refusal) -> Self {
        match refusal {
            Refusal::InvalidUrl | Refusal::InvalidHeader => Self::InvalidArguments,
            Refusal::HttpsRequired => Self::HttpsRequired,
            Refusal::MethodNotAllowed => Self::MethodNotAllowed,
            Refusal::HostNotAllowed => Self::HostNotAllowed,
            Refusal::AddressNotAllowed => Self::AddressNotAllowed,
            Refusal::PlacementNotAllowed => Self::CredentialPlacementNotAllowed,
            Refusal::CredentialHostNotAllowed => Self::CredentialHostNotAllowed,
            Refusal::SecretNotFound => Self::SecretNotFound,
            Refusal::RequestPolicyNotAllowed => Self::RequestPolicyNotAllowed,
            Refusal::CredentialMintRefused => Self::CredentialMintRefused,
            Refusal::UpstreamUnreachable => Self::UpstreamUnreachable,
        }
    }
}

/// `draft`, admitted by the lease's guard and sent through `transport`, its
/// answer masked for every token the lease minted; a refusal is already the
/// output the model reads.
pub(crate) async fn send(
    entry: &Entry,
    transport: &dyn Transport,
    lease: &Lease<'_>,
    draft: Draft,
) -> Result<Inbound, ToolOutput> {
    // The guard is held to admit and mint, never across the send, so a slow
    // upstream holds no other call of the lease.
    let prepared = lease.egress.lock().await.prepare(draft).await;
    let outbound = prepared.map_err(|failure| refused(entry, lease.lease_id, &failure))?;
    let mut inbound = transport
        .send(outbound)
        .await
        .map_err(|failure| refused(entry, lease.lease_id, &failure))?;
    inbound.body = masked(lease, inbound.body).await;
    Ok(inbound)
}

/// `text` with every token the lease minted masked; `text` itself when none
/// is in it.
pub(crate) async fn masked(lease: &Lease<'_>, text: String) -> String {
    let changed = match lease.egress.lock().await.mask(&text) {
        Cow::Owned(masked) => Some(masked),
        Cow::Borrowed(_) => None,
    };
    changed.unwrap_or(text)
}

/// `text` as the call's output: succeeded on a 2xx, failed with
/// `upstream_status` otherwise, the model reading the same text either way.
pub(crate) fn answered(status: u16, text: String) -> ToolOutput {
    if SUCCESS.contains(&status) {
        ToolOutput::succeeded(text)
    } else {
        ToolOutput::failed(ToolErrorCode::UpstreamStatus, &text)
    }
}

/// A refusal as the model reads it, its code first and its sentence after,
/// logged as `tool_refused` under lease `lease_id`. A client that was never
/// built sends nothing, so it reads as a request that got no answer.
pub(crate) fn refused(entry: &Entry, lease_id: &str, failure: &Error) -> ToolOutput {
    let code = failure
        .refusal()
        .map_or(ToolErrorCode::UpstreamUnreachable, ToolErrorCode::from);
    let tool = entry.name();
    let error_code = code.as_str();
    let event = EVENT_TOOL_REFUSED;
    tracing::info!(lease_id, tool, error_code, event);
    ToolOutput::failed(code, &failure.detail())
}

#[cfg(test)]
#[path = "egress/tests.rs"]
mod tests;
