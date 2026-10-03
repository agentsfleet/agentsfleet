//! rig's HTTP seam, served by the runner's own client.
//!
//! rig builds each request and reads each reply; this sends it. That keeps the
//! transport the runner's: one client that follows no redirect, its timeouts,
//! and the bounded retry around every send, so rig never sees a failure a
//! second send could fix and the key never reaches a host the lease was not
//! admitted for. A refusal is handed back as it arrived, so rig keeps its
//! status, its headers and the provider's own error code.

use std::fmt;
use std::sync::Arc;

use afd_core::clock::saturating_millis;
use bytes::Bytes;
use futures_util::TryStreamExt as _;
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
                let bytes = response.bytes().await.map_err(rig_http::Error::instance)?;
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
            let chunks = response.bytes_stream().map_err(rig_http::Error::instance);
            let stream: BoxedStream = Box::pin(chunks);
            builder.body(stream).map_err(rig_http::Error::Protocol)
        }
    }
}
