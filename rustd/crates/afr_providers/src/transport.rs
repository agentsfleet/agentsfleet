//! rig's HTTP seam, served by the runner's own client.
//!
//! rig builds each request and reads each reply; this sends it. That keeps the
//! transport the runner's: one client that follows no redirect, its timeouts,
//! and the bounded retry around every send, so rig never sees a failure a
//! second send could fix and the key never reaches a host the lease was not
//! admitted for. A refusal is handed back as it arrived, so rig keeps its
//! status, its headers and the provider's own error code. Every reply is read
//! under [`REPLY_MAX_BYTES`]: past it the read ends on [`Oversize`], so one
//! endpoint cannot grow a shared runner's memory with one turn.

use std::fmt;
use std::sync::Arc;

use afd_core::clock::saturating_millis;
use bytes::{Bytes, BytesMut};
use futures_util::{Stream, StreamExt as _, TryStreamExt as _};
use rig_core::http_client::{
    self as rig_http, BoxedStream, HttpClientExt, LazyBody, MultipartForm, Request, Response,
    StreamingResponse,
};
use rig_core::wasm_compat::WasmCompatSend;

use crate::retry::{self, Retrying};

/// The log line a retried send writes.
const EVENT_RETRY: &str = "provider_retry";
/// Why a multipart send is refused: no wire this runner speaks uploads files.
const NO_MULTIPART: &str = "the model providers' transport sends no multipart body";

/// The most bytes one reply may carry: one turn, framed as its wire frames it.
///
/// A Responses or Chat turn names no output limit and frames a token or two
/// in a few hundred bytes of event, so this holds a reply of about a hundred
/// thousand tokens.
pub const REPLY_MAX_BYTES: usize = 32 * 1024 * 1024;

/// The read's refusal of a reply past [`REPLY_MAX_BYTES`], which the turn
/// reads back out of rig's error so the reply is never asked for again.
#[derive(Debug, thiserror::Error)]
#[error("the model provider's reply passed {} bytes", REPLY_MAX_BYTES)]
pub(crate) struct Oversize;

/// Why a reply could not be read: the transport's error, or [`Oversize`].
/// Boxed, as rig boxes it, and lifted into rig's error at the seam.
type Unread = Box<dyn std::error::Error + Send + Sync>;

/// One lease's transport: the shared client, and whose sends it is making.
#[derive(Clone)]
pub(crate) struct Transport {
    client: reqwest::Client,
    whose: Arc<Whose>,
}

/// Whose sends a transport makes, for its retry line.
struct Whose {
    lease_id: Box<str>,
    provider: Box<str>,
}

impl fmt::Debug for Transport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Transport")
            .field("lease_id", &self.whose.lease_id)
            .field("provider", &self.whose.provider)
            .finish_non_exhaustive()
    }
}

impl Transport {
    /// Lease `lease_id`'s transport to `provider`, over `client`.
    pub(crate) fn new(client: reqwest::Client, lease_id: &str, provider: &str) -> Self {
        let whose = Whose {
            lease_id: lease_id.into(),
            provider: provider.into(),
        };
        Self {
            client,
            whose: Arc::new(whose),
        }
    }

    /// Sends `parts` with `body` under the bounded retry. Each attempt gets
    /// its own builder, the body shared rather than copied.
    async fn send_retrying(
        &self,
        parts: &http::request::Parts,
        body: &Bytes,
    ) -> reqwest::Result<reqwest::Response> {
        let build = || {
            self.client
                .request(parts.method.clone(), parts.uri.to_string())
                .headers(parts.headers.clone())
                .body(body.clone())
        };
        retry::send(build, |retry| self.retrying(retry)).await
    }

    /// Logs a send about to be retried.
    fn retrying(&self, retry: Retrying) {
        let lease_id = &*self.whose.lease_id;
        let provider = &*self.whose.provider;
        let status = retry.status;
        let attempt = retry.attempt;
        let wait_ms = saturating_millis(retry.wait);
        let event = EVENT_RETRY;
        tracing::warn!(lease_id, provider, status, attempt, wait_ms, event);
    }
}

/// The status and headers of `response`, for the reply rig reads.
fn head(response: &reqwest::Response) -> http::response::Builder {
    let mut builder = Response::builder().status(response.status());
    if let Some(headers) = builder.headers_mut() {
        headers.clone_from(response.headers());
    }
    builder
}

/// `response`'s body as it arrives, ended on [`Oversize`] once it has
/// carried more than `cap` bytes.
fn capped(
    response: reqwest::Response,
    cap: usize,
) -> impl Stream<Item = Result<Bytes, Unread>> + Send + 'static {
    let mut read = 0_usize;
    response.bytes_stream().map(move |chunk| {
        let chunk = chunk?;
        read = read.saturating_add(chunk.len());
        if read > cap {
            return Err(Oversize.into());
        }
        Ok(chunk)
    })
}

/// `response`'s whole body, refused on [`Oversize`] past `cap` bytes.
async fn whole(response: reqwest::Response, cap: usize) -> Result<Bytes, Unread> {
    capped(response, cap)
        .try_fold(BytesMut::new(), |mut body, chunk| {
            body.extend_from_slice(&chunk);
            std::future::ready(Ok(body))
        })
        .await
        .map(BytesMut::freeze)
}

impl HttpClientExt for Transport {
    fn send<T, U>(
        &self,
        req: Request<T>,
    ) -> impl Future<Output = rig_http::Result<Response<LazyBody<U>>>> + WasmCompatSend + 'static
    where
        T: Into<Bytes> + WasmCompatSend,
        U: From<Bytes> + WasmCompatSend + 'static,
    {
        let (parts, body) = req.into_parts();
        let body: Bytes = body.into();
        let transport = self.clone();
        async move {
            let sent = transport.send_retrying(&parts, &body).await;
            let response = sent.map_err(rig_http::Error::instance)?;
            let builder = head(&response);
            let read: LazyBody<U> = Box::pin(async move {
                let bytes =
                    (whole(response, REPLY_MAX_BYTES).await).map_err(rig_http::Error::Instance)?;
                Ok(U::from(bytes))
            });
            builder.body(read).map_err(rig_http::Error::Protocol)
        }
    }

    fn send_multipart<U>(
        &self,
        _req: Request<MultipartForm>,
    ) -> impl Future<Output = rig_http::Result<Response<LazyBody<U>>>> + WasmCompatSend + 'static
    where
        U: From<Bytes> + WasmCompatSend + 'static,
    {
        let refused = rig_http::Error::instance(std::io::Error::other(NO_MULTIPART));
        std::future::ready(Err(refused))
    }

    fn send_streaming<T>(
        &self,
        req: Request<T>,
    ) -> impl Future<Output = rig_http::Result<StreamingResponse>> + WasmCompatSend
    where
        T: Into<Bytes> + WasmCompatSend,
    {
        let (parts, body) = req.into_parts();
        let body: Bytes = body.into();
        let transport = self.clone();
        async move {
            let sent = transport.send_retrying(&parts, &body).await;
            let response = sent.map_err(rig_http::Error::instance)?;
            let builder = head(&response);
            let chunks = capped(response, REPLY_MAX_BYTES).map_err(rig_http::Error::Instance);
            let stream: BoxedStream = Box::pin(chunks);
            builder.body(stream).map_err(rig_http::Error::Protocol)
        }
    }
}

#[cfg(test)]
#[path = "transport/tests.rs"]
mod tests;
