//! What the three egress tools share: sending through the lease's guard, and
//! how a refusal and an answer read to the model.
//!
//! `http_request`, `web_fetch` and `pushover` differ only in the request they
//! draft and how they word the answer; admission, the transport, the mask and
//! the refusal's code and log line are the same for each, so they live here
//! once.

use std::borrow::Cow;
use std::sync::Arc;

use afr_egress::{Draft, Inbound, Refusal, Transport};

use crate::catalog::Entry;
use crate::lease::Lease;
use crate::runtime::{ToolErrorCode, ToolOutput};

/// The event a refused call logs under: the Zig bridge's spelling.
const EVENT_TOOL_REFUSED: &str = "tool_refused";

/// The statuses a tool reports as succeeded.
const SUCCESS: std::ops::Range<u16> = 200..300;

/// The transport every egress tool sends through, shared across leases.
pub(crate) type SharedTransport = Arc<dyn Transport>;

impl From<&Refusal> for ToolErrorCode {
    fn from(refusal: &Refusal) -> Self {
        match refusal {
            Refusal::InvalidUrl { .. } | Refusal::InvalidHeader { .. } => Self::InvalidArguments,
            Refusal::HttpsRequired => Self::HttpsRequired,
            Refusal::MethodNotAllowed { .. } => Self::MethodNotAllowed,
            Refusal::HostNotAllowed { .. } => Self::HostNotAllowed,
            Refusal::AddressNotAllowed { .. } => Self::AddressNotAllowed,
            Refusal::PlacementNotAllowed { .. } => Self::CredentialPlacementNotAllowed,
            Refusal::CredentialHostNotAllowed { .. } => Self::CredentialHostNotAllowed,
            Refusal::SecretNotFound { .. } => Self::SecretNotFound,
            Refusal::RequestPolicyNotAllowed { .. } => Self::RequestPolicyNotAllowed,
            Refusal::CredentialMintRefused { .. } => Self::CredentialMintRefused,
            Refusal::UpstreamUnreachable { .. } => Self::UpstreamUnreachable,
        }
    }
}

/// `draft`, admitted by the lease's guard and sent through `transport`, its
/// answer masked for every token the lease minted; a refusal is already the
/// output the model reads.
pub(crate) async fn send(
    entry: &Entry,
    transport: &dyn Transport,
    lease: &mut Lease<'_>,
    draft: Draft,
) -> Result<Inbound, ToolOutput> {
    let outbound = lease
        .egress
        .prepare(draft)
        .await
        .map_err(|refusal| refused(entry, &refusal))?;
    let mut inbound = transport
        .send(outbound)
        .await
        .map_err(|refusal| refused(entry, &refusal))?;
    inbound.body = masked(lease, inbound.body);
    Ok(inbound)
}

/// `text` with every token the lease minted masked; `text` itself when none
/// is in it.
pub(crate) fn masked(lease: &Lease<'_>, text: String) -> String {
    let changed = match lease.egress.mask(&text) {
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

/// A refusal as the model reads it, logged as `tool_refused`.
pub(crate) fn refused(entry: &Entry, refusal: &Refusal) -> ToolOutput {
    let code = ToolErrorCode::from(refusal);
    let tool = entry.name();
    let error_code = code.as_str();
    let event = EVENT_TOOL_REFUSED;
    tracing::info!(tool, error_code, event);
    ToolOutput::failed(code, &refusal.to_string())
}

#[cfg(test)]
#[path = "egress/tests.rs"]
mod tests;
