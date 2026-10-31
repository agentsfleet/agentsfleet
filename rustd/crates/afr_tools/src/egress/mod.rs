//! What the three egress tools share: sending through the lease's guard, and
//! how a refusal and an answer read to the model.
//!
//! `http_request`, `web_fetch` and `pushover` differ only in the request they
//! draft and how they word the answer; admission, the transport, the mask and
//! the refusal's code and log line are the same for each, so they live here
//! once.

use std::borrow::Cow;
use std::sync::Arc;

use afr_egress::REQUEST_TIMEOUT;
use afr_egress::redirect::{Chain, Stop};
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
    mask_response(lease, &mut inbound).await;
    Ok(inbound)
}

/// A bodyless GET's whole admission/mint/connect/read chain shares one deadline.
pub(crate) async fn send_get(
    entry: &Entry,
    transport: &dyn Transport,
    lease: &Lease<'_>,
    draft: Draft,
) -> Result<Inbound, ToolOutput> {
    tokio::time::timeout(REQUEST_TIMEOUT, get_chain(entry, transport, lease, draft))
        .await
        .unwrap_or_else(|_expired| {
            Err(ToolOutput::failed(
                ToolErrorCode::UpstreamUnreachable,
                "HTTP read deadline exceeded",
            ))
        })
}

async fn get_chain(
    entry: &Entry,
    transport: &dyn Transport,
    lease: &Lease<'_>,
    draft: Draft,
) -> Result<Inbound, ToolOutput> {
    let headers = draft.headers.clone();
    let mut outbound = lease
        .egress
        .lock()
        .await
        .prepare(draft)
        .await
        .map_err(|failure| refused(entry, lease.lease_id, &failure))?;
    let mut chain = Chain::new(outbound.url(), headers);
    loop {
        let mut inbound = transport
            .send(outbound.for_redirect_read())
            .await
            .map_err(|failure| redirect_refused(entry, lease, &chain, &failure))?;
        let next = chain
            .next(&mut inbound)
            .map_err(|reason| stopped(reason, &chain, &inbound))?;
        let Some(draft) = next else {
            mask_response(lease, &mut inbound).await;
            chain.finish();
            return Ok(inbound);
        };
        // Every target is re-admitted; a network wait never holds this lease guard.
        outbound = lease
            .egress
            .lock()
            .await
            .prepare(draft)
            .await
            .map_err(|failure| redirect_refused(entry, lease, &chain, &failure))?;
    }
}

fn stopped(reason: Stop, chain: &Chain, inbound: &Inbound) -> ToolOutput {
    let code = if reason == Stop::InvalidTarget {
        ToolErrorCode::InvalidArguments
    } else {
        ToolErrorCode::UpstreamUnreachable
    };
    let location = inbound
        .location
        .as_deref()
        .map(|origin| format!("Location: {origin}\n"))
        .unwrap_or_default();
    ToolOutput::failed(
        code,
        &format!(
            "Status: {}\n{location}Redirects followed: {}\n{}",
            chain.status(),
            chain.hops(),
            reason.reason()
        ),
    )
}

fn redirect_refused(
    entry: &Entry,
    lease: &Lease<'_>,
    chain: &Chain,
    failure: &Error,
) -> ToolOutput {
    if !chain.started() {
        return refused(entry, lease.lease_id, failure);
    }
    let code = log_refusal(entry, lease.lease_id, failure);
    ToolOutput::failed(
        code,
        &format!(
            "Status: {}\nLocation: {}\nRedirect request did not complete",
            chain.status(),
            chain.origin()
        ),
    )
}

async fn mask_response(lease: &Lease<'_>, inbound: &mut Inbound) {
    inbound.body = masked(lease, std::mem::take(&mut inbound.body)).await;
    if let Some(content_type) = inbound.content_type.take() {
        inbound.content_type = Some(masked(lease, content_type).await);
    }
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
    let code = log_refusal(entry, lease_id, failure);
    ToolOutput::failed(code, &failure.detail())
}

fn log_refusal(entry: &Entry, lease_id: &str, failure: &Error) -> ToolErrorCode {
    let code = failure
        .refusal()
        .map_or(ToolErrorCode::UpstreamUnreachable, ToolErrorCode::from);
    let tool = entry.name();
    let error_code = code.as_str();
    let event = EVENT_TOOL_REFUSED;
    tracing::info!(lease_id, tool, error_code, event);
    code
}

#[cfg(test)]
mod tests;
