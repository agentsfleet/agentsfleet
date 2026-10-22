//! The same bounded HTTP seam for daemon and runner; send/retry/egress policy stays with callers.

use std::fmt::Debug;
use std::sync::{
    Arc,
    atomic::{AtomicU16, Ordering},
};

use bytes::{Bytes, BytesMut};
use futures_util::{TryStreamExt as _, future::BoxFuture};
use rig_core::http_client::{
    self as rig_http, HttpClientExt, LazyBody, MultipartForm, Request, Response, StreamingResponse,
};
use rig_core::wasm_compat::WasmCompatSend;

use crate::{Error, Result, error::ErrorKind};

/// Wire framing overhead is bounded separately from each purpose's normalized reply envelope.
pub const REPLY_MAX_BYTES: usize = 32 * 1024 * 1024;

/// A guarded physical sender. One invocation sends once unless its explicit runtime policy retries.
pub trait SendRequest: Send + Sync + Debug {
    /// Sends bytes and returns the response body without collecting it.
    fn send(
        &self,
        request: Request<Bytes>,
    ) -> BoxFuture<'static, rig_http::Result<StreamingResponse>>;
}

#[cfg(test)]
mod tests;

/// Shared SDK HTTP adapter. Diagnostics never include the request or response body.
#[derive(Clone)]
pub struct HttpTransport {
    physical: Arc<dyn SendRequest>,
    status: Arc<AtomicU16>,
}

impl Debug for HttpTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpTransport")
            .field("status", &self.status())
            .finish_non_exhaustive()
    }
}

impl HttpTransport {
    /// Wraps the caller's authenticated, guarded physical sender.
    #[must_use]
    pub fn new(physical: Arc<dyn SendRequest>) -> Self {
        Self {
            physical,
            status: Arc::new(AtomicU16::new(0)),
        }
    }

    /// Observed HTTP status; zero means no response head arrived.
    #[must_use]
    pub fn status(&self) -> u16 {
        self.status.load(Ordering::Relaxed)
    }

    async fn streaming(
        &self,
        request: Request<Bytes>,
    ) -> Result<StreamingResponse, Box<rig_http::Error>> {
        let response = self.physical.send(request).await?;
        self.status
            .store(response.status().as_u16(), Ordering::Relaxed);
        let (parts, body) = response.into_parts();
        let body =
            futures_util::stream::try_unfold((body, 0_usize), |(mut body, read)| async move {
                let Some(chunk) = body.try_next().await? else {
                    return Ok(None);
                };
                let read = read.saturating_add(chunk.len());
                if read > REPLY_MAX_BYTES {
                    return Err(rig_http::Error::instance(Error::from(
                        ErrorKind::ReplyTooLarge,
                    )));
                }
                Ok(Some((chunk, (body, read))))
            });
        Ok(Response::from_parts(parts, Box::pin(body)))
    }
}

impl HttpClientExt for HttpTransport {
    fn send<T, U>(
        &self,
        req: Request<T>,
    ) -> impl Future<Output = rig_http::Result<Response<LazyBody<U>>>> + WasmCompatSend + 'static
    where
        T: Into<Bytes> + WasmCompatSend,
        U: From<Bytes> + WasmCompatSend + 'static,
    {
        let transport = self.clone();
        let request = req.map(Into::into);
        async move {
            let response = transport
                .streaming(request)
                .await
                .map_err(|source| *source)?;
            let (parts, body) = response.into_parts();
            let read: LazyBody<U> = Box::pin(async move {
                body.try_fold(BytesMut::new(), |mut body, chunk| async move {
                    body.extend_from_slice(&chunk);
                    Ok(body)
                })
                .await
                .map(|body| U::from(body.freeze()))
            });
            Ok(Response::from_parts(parts, read))
        }
    }

    fn send_multipart<U>(
        &self,
        _request: Request<MultipartForm>,
    ) -> impl Future<Output = rig_http::Result<Response<LazyBody<U>>>> + WasmCompatSend + 'static
    where
        U: From<Bytes> + WasmCompatSend + 'static,
    {
        std::future::ready(Err(rig_http::Error::instance(Error::from(
            ErrorKind::Multipart,
        ))))
    }

    fn send_streaming<T>(
        &self,
        req: Request<T>,
    ) -> impl Future<Output = rig_http::Result<StreamingResponse>> + WasmCompatSend
    where
        T: Into<Bytes> + WasmCompatSend,
    {
        let transport = self.clone();
        let request = req.map(Into::into);
        async move { transport.streaming(request).await.map_err(|source| *source) }
    }
}
