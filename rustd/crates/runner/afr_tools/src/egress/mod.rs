//! The run's egress guard as `guffbolt`'s outbound hook: every request a
//! generic tool drafts is admitted, sent and masked here.
//!
//! `http_request`, `web_fetch` and the Pushover notifier behind `notify`
//! differ only in the request they draft and how they word the answer;
//! admission, the transport, the mask and the refusal's code and log line are
//! the guard's, so they live here once, and an MCP connection is admitted by
//! the same policy.

use std::borrow::Cow;
use std::pin::Pin;
use std::sync::Arc;

use af_egress::REQUEST_TIMEOUT;
use af_egress::redirect::{Chain, Stop};
use af_egress::{Draft, Error, Inbound, Placement, Refusal, Transport};
use guffbolt::tools::code;
use guffbolt::tools::outbound::{
    Admitted, Outbound, OutboundFuture, Placement as Placing, Redirects, Refused, Request, Response,
};

use crate::run::Run;
use crate::runtime::ToolErrorCode;

/// The event a refused call logs under, fixed because dashboards match on it.
const EVENT_TOOL_REFUSED: &str = "tool_refused";
const READ_DEADLINE_EXCEEDED: &str = "HTTP read deadline exceeded";
/// What a request from a tool built to send nothing reads back.
const NO_TRANSPORT: &str = "this tool sends no request";
/// The method an MCP connection is admitted as.
const CONNECT_METHOD: &str = "GET";

// A body is cut at the guard's figure, which the harness words its notice by.
const _: () =
    assert!(guffbolt::tools::outbound::RESPONSE_MAX_BYTES == af_egress::RESPONSE_MAX_BYTES);

/// The transport every egress tool sends through, shared across runs.
pub(crate) type SharedTransport = Arc<dyn Transport>;

impl From<Refusal> for ToolErrorCode {
    fn from(refusal: Refusal) -> Self {
        match refusal {
            Refusal::InvalidUrl | Refusal::InvalidHeader => Self::InvalidArguments,
            Refusal::HttpsRequired => Self::HttpsRequired,
            Refusal::MethodNotAllowed => Self::MethodNotAllowed,
            Refusal::HostNotAllowed => Self::HostNotAllowed,
            Refusal::EgressDenied => Self::EgressDenied,
            Refusal::PlacementNotAllowed => Self::CredentialPlacementNotAllowed,
            Refusal::CredentialHostNotAllowed => Self::CredentialHostNotAllowed,
            Refusal::SecretNotFound => Self::SecretNotFound,
            Refusal::RequestPolicyNotAllowed => Self::RequestPolicyNotAllowed,
            Refusal::CredentialMintRefused => Self::CredentialMintRefused,
            Refusal::UpstreamUnreachable => Self::UpstreamUnreachable,
        }
    }
}

/// One call's view of the run's guard and the shared transport.
#[derive(Debug)]
pub(crate) struct Egressed<'l, 'run> {
    transport: Option<&'l dyn Transport>,
    run: &'l Run<'run>,
}

impl<'l, 'run> Egressed<'l, 'run> {
    /// The guard of `run`, sending through `transport`.
    pub(crate) const fn new(transport: Option<&'l dyn Transport>, run: &'l Run<'run>) -> Self {
        Self { transport, run }
    }
}

impl Outbound for Egressed<'_, '_> {
    fn send<'a>(&'a self, tool: &'a str, request: Request) -> OutboundFuture<'a, Response> {
        Box::pin(async move {
            let Some(transport) = self.transport else {
                return Err(Refused::new(code::UPSTREAM_UNREACHABLE, NO_TRANSPORT));
            };
            let redirects = request.redirects;
            let draft = Draft {
                method: request.method,
                url: request.url,
                headers: request.headers,
                body: request.body,
                placement: match request.placement {
                    Placing::Authorization => Placement::Authorization,
                    Placing::Nowhere => Placement::Nowhere,
                },
            };
            let inbound = match redirects {
                Redirects::Follow => send_get(tool, transport, self.run, draft).await,
                Redirects::Stop => send(tool, transport, self.run, draft).await,
            }?;
            Ok(Response {
                status: inbound.status,
                location: inbound.location,
                content_type: inbound.content_type,
                body: inbound.body,
                truncated: inbound.truncated,
                lossy: inbound.lossy,
                followed_redirects: inbound.followed_redirects,
            })
        })
    }

    fn mask(&self, text: String) -> Pin<Box<dyn Future<Output = String> + Send + '_>> {
        Box::pin(masked(self.run, text))
    }

    fn secret<'a>(
        &'a self,
        tool: &'a str,
        name: &'a str,
        field: &'a str,
    ) -> OutboundFuture<'a, String> {
        Box::pin(async move {
            let statics = self.run.egress.lock().await.statics();
            statics
                .field(name, field)
                .map(str::to_owned)
                .ok_or_else(|| {
                    refused(tool, self.run.run_id, &Error::secret_not_found(name, field))
                })
        })
    }

    fn connect<'a>(&'a self, tool: &'a str, url: &'a str) -> OutboundFuture<'a, Admitted> {
        Box::pin(async move {
            let draft = Draft {
                method: CONNECT_METHOD.to_owned(),
                url: url.to_owned(),
                headers: Vec::new(),
                body: None,
                placement: Placement::Nowhere,
            };
            let prepared = self.run.egress.lock().await.prepare(draft).await;
            let outbound = prepared.map_err(|failure| refused(tool, self.run.run_id, &failure))?;
            Ok(Admitted {
                url: outbound.url().to_string(),
                headers: Vec::new(),
            })
        })
    }
}

/// `draft`, admitted by the run's guard and sent through `transport`, its
/// answer masked for every token the run minted.
async fn send(
    tool: &str,
    transport: &dyn Transport,
    run: &Run<'_>,
    draft: Draft,
) -> Result<Inbound, Refused> {
    // The guard is held to admit and mint, never across the send, so a slow
    // upstream holds no other call of the run.
    let prepared = run.egress.lock().await.prepare(draft).await;
    let outbound = prepared.map_err(|failure| refused(tool, run.run_id, &failure))?;
    let mut inbound = transport
        .send(outbound)
        .await
        .map_err(|failure| refused(tool, run.run_id, &failure))?;
    mask_response(run, &mut inbound).await;
    Ok(inbound)
}

/// A bodyless GET's whole admission/mint/connect/read chain shares one deadline.
async fn send_get(
    tool: &str,
    transport: &dyn Transport,
    run: &Run<'_>,
    draft: Draft,
) -> Result<Inbound, Refused> {
    tokio::time::timeout(REQUEST_TIMEOUT, get_chain(tool, transport, run, draft))
        .await
        .unwrap_or_else(|_expired| Err(read_expired(tool, run.run_id)))
}

fn read_expired(tool: &str, run_id: &str) -> Refused {
    let error_code = af_core::error_code::INTERNAL_OPERATION_FAILED.as_str();
    let reason = READ_DEADLINE_EXCEEDED;
    {
        let event = &"http_read_deadline_exceeded";
        tracing::warn!(event, run_id, tool, error_code, reason);
    };
    Refused::new(
        ToolErrorCode::UpstreamUnreachable.into(),
        READ_DEADLINE_EXCEEDED,
    )
}

async fn get_chain(
    tool: &str,
    transport: &dyn Transport,
    run: &Run<'_>,
    draft: Draft,
) -> Result<Inbound, Refused> {
    let headers = draft.headers.clone();
    let mut outbound = run
        .egress
        .lock()
        .await
        .prepare(draft)
        .await
        .map_err(|failure| refused(tool, run.run_id, &failure))?;
    let mut chain = Chain::new(outbound.url(), headers);
    loop {
        let mut inbound = transport
            .send(outbound.for_redirect_read())
            .await
            .map_err(|failure| redirect_refused(tool, run, &chain, &failure))?;
        let next = chain
            .next(&mut inbound)
            .map_err(|reason| stopped(reason, &chain, &inbound))?;
        let Some(draft) = next else {
            mask_response(run, &mut inbound).await;
            chain.finish();
            return Ok(inbound);
        };
        // Every target is re-admitted; a network wait never holds this run guard.
        outbound = run
            .egress
            .lock()
            .await
            .prepare(draft)
            .await
            .map_err(|failure| redirect_refused(tool, run, &chain, &failure))?;
    }
}

fn stopped(reason: Stop, chain: &Chain, inbound: &Inbound) -> Refused {
    let code = if reason == Stop::InvalidTarget {
        ToolErrorCode::InvalidArguments
    } else {
        ToolErrorCode::UpstreamUnreachable
    };
    let location = inbound
        .location
        .as_deref()
        .map_or_default(|origin| format!("Location: {origin}\n"));
    Refused::new(
        code.into(),
        format!(
            "Status: {}\n{location}Redirects followed: {}\n{}",
            chain.status(),
            chain.hops(),
            reason.reason()
        ),
    )
}

fn redirect_refused(tool: &str, run: &Run<'_>, chain: &Chain, failure: &Error) -> Refused {
    if !chain.started() {
        return refused(tool, run.run_id, failure);
    }
    let code = log_refusal(tool, run.run_id, failure);
    Refused::new(
        code.into(),
        format!(
            "Status: {}\nLocation: {}\nRedirect request did not complete",
            chain.status(),
            chain.origin()
        ),
    )
}

async fn mask_response(run: &Run<'_>, inbound: &mut Inbound) {
    inbound.body = masked(run, std::mem::take(&mut inbound.body)).await;
    if let Some(content_type) = inbound.content_type.take() {
        inbound.content_type = Some(masked(run, content_type).await);
    }
}

/// `text` with every token the run minted masked; `text` itself when none
/// is in it.
pub(crate) async fn masked(run: &Run<'_>, text: String) -> String {
    let mask = run.egress.lock().await.mask(&text);
    let changed = match mask {
        Cow::Owned(masked) => Some(masked),
        Cow::Borrowed(_) => None,
    };
    changed.unwrap_or(text)
}

/// A refusal as the model reads it, its code first and its sentence after,
/// logged as `tool_refused` under run `run_id`. A client that was never
/// built sends nothing, so it reads as a request that got no answer.
pub(crate) fn refused(tool: &str, run_id: &str, failure: &Error) -> Refused {
    let code = log_refusal(tool, run_id, failure);
    Refused::new(code.into(), failure.detail())
}

fn log_refusal(tool: &str, run_id: &str, failure: &Error) -> ToolErrorCode {
    let code = failure
        .refusal()
        .map_or(ToolErrorCode::UpstreamUnreachable, ToolErrorCode::from);
    let error_code = code.as_str();
    let event = EVENT_TOOL_REFUSED;
    tracing::info!(run_id, tool, error_code, event);
    code
}

#[cfg(test)]
mod tests;
